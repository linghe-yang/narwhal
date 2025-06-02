
use log::{info};
use std::collections::{HashMap, HashSet};
use curve25519_dalek::Scalar;
use tokio::sync::mpsc::{Receiver, Sender};
use config::Committee;
use crypto::{Digest, PublicKey};
use model::types_and_const::{Epoch, RandomNum};
use crate::breeze_origin::breeze_reconstruct_dealer::BreezeReconResult;
use crate::breeze_structs::{BreezeContent, BreezeMessage};
use crate::Secret;

pub struct BreezeResult {
    committee: Committee,
    breeze_recon_certificate_receiver: Receiver<(HashSet<Digest>,Epoch, usize)>,
    breeze_reconstruct_secret_receiver: Receiver<BreezeMessage>,
    breeze_result_sender: Sender<(Epoch, usize, RandomNum)>,

    certificates_to_reconstruct_buffer: Vec<(HashSet<Digest>, Epoch, usize)>,
    shares_to_cumulate: HashMap<(Epoch, usize), HashMap<Digest, HashSet<(PublicKey,Secret)>>>,
    reconstructed_epoch_wave: Vec<(Epoch, usize)>
}

impl BreezeResult {
    pub fn spawn(
        committee: Committee,
        breeze_recon_certificate_receiver: Receiver<(HashSet<Digest>,Epoch, usize)>,
        breeze_reconstruct_secret_receiver: Receiver<BreezeMessage>,
        breeze_result_sender: Sender<(Epoch, usize, RandomNum)>,
    ) {
        tokio::spawn(async move {
            Self {
                committee,
                breeze_recon_certificate_receiver,
                breeze_reconstruct_secret_receiver,
                breeze_result_sender,
                certificates_to_reconstruct_buffer: Vec::new(),
                shares_to_cumulate: HashMap::new(),
                reconstructed_epoch_wave: Vec::new()
            }
            .run()
            .await;
        });
    }

    pub async fn run(&mut self) {
        info!("Breeze result start to listen");
        let threshold = self.committee.authorities_fault_tolerance() + 1;
        loop {
            tokio::select! {
                Some(certificates_to_reconstruct) = self.breeze_recon_certificate_receiver.recv() => {
                    let key = (certificates_to_reconstruct.1, certificates_to_reconstruct.2);
                    if !self.reconstructed_epoch_wave.contains(&key) {
                        let exists = self.certificates_to_reconstruct_buffer.iter().any(|(_, e, w)| e == &certificates_to_reconstruct.1 && w == &certificates_to_reconstruct.2);
                        if !exists {
                            self.certificates_to_reconstruct_buffer.push(certificates_to_reconstruct);
                        }
                    }
                },
                Some(shares_from_others) = self.breeze_reconstruct_secret_receiver.recv() => {
                    match shares_from_others.content {
                        BreezeContent::Reconstruct(share) => {
                            let key = (share.epoch, share.index);
                            let shares_vec = self.shares_to_cumulate
                                .entry(key)
                                .or_insert_with(HashMap::new);
                            for single_share in share.secrets {
                                if !single_share.verify(){
                                    continue;
                                }
                                let scalar_set = shares_vec
                                    .entry(single_share.c)
                                    .or_insert_with(HashSet::new);
                                scalar_set.insert((shares_from_others.sender,single_share.y));
                            }
                        }
                        _ => {}
                    }
                }
            }

            let mut secrets_to_reconstruct = Vec::new();
            self.certificates_to_reconstruct_buffer.retain(|(digests, epoch, index)| {
                let mut digest_can_be_reconstructed = HashSet::new();
                let mut secret_can_be_reconstructed = Vec::new();
                let key = (*epoch, *index);
                if let Some(shares) = self.shares_to_cumulate.get(&key) {
                    for (c,s) in shares.iter() {
                        if s.len() >= threshold {
                            let s: Vec<(PublicKey,Secret)> = s.iter().cloned().collect();
                            secret_can_be_reconstructed.push((*c, s));
                            digest_can_be_reconstructed.insert(*c);
                        }
                    }
                    return if &digest_can_be_reconstructed == digests {
                        secrets_to_reconstruct.push((*epoch, *index, secret_can_be_reconstructed));
                        self.reconstructed_epoch_wave.push(key);
                        self.shares_to_cumulate.remove(&key);
                        false
                    } else {
                        true
                    }
                }
                true
            });

            for (epoch, index, secret_set) in secrets_to_reconstruct {
                let mut cumulated_output = Scalar::ZERO;
                for (_,shares) in secret_set {
                    let mut ids = Vec::new();
                    let mut values = Vec::new();
                    for (pk,value) in shares{
                        let id = self.committee.get_id(&pk).unwrap();
                        ids.push(id);
                        values.push(value);
                    }
                    cumulated_output += BreezeReconResult::interpolate(&ids, &values);
                }
                let recon_output = BreezeReconResult::new(cumulated_output);
                self.breeze_result_sender.send((epoch, index, recon_output.secret_to_number()))
                    .await
                    .expect("breeze_result_sender error to send");
            }
        }
    }
}
