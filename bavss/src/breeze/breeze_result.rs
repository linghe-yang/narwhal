
use curve25519_dalek::Scalar;
use log::{info};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use config::Committee;
use crypto::{Digest, PublicKey};
use model::breeze_structs::{BreezeContent, BreezeMessage, ReconstructShare, SingleShare};
use model::scale_type::{Epoch, RandomNum};
use crate::breeze::breeze_reconstruct_dealer::BreezeReconResult;

pub struct BreezeResult {
    committee: Arc<RwLock<Committee>>,
    breeze_recon_certificate_receiver: Receiver<(HashSet<Digest>,Epoch, usize)>,
    breeze_reconstruct_secret_receiver: Receiver<BreezeMessage>,
    breeze_result_sender: Sender<(Epoch, usize, RandomNum)>,

    certificates_to_reconstruct_buffer: Vec<(HashSet<Digest>, Epoch, usize)>,
    shares_to_cumulate: HashMap<(Epoch, usize), HashMap<Digest, HashSet<(PublicKey,Scalar)>>>,
    reconstructed_epoch_wave: Vec<(Epoch, usize)>
}

impl BreezeResult {
    pub fn spawn(
        committee: Arc<RwLock<Committee>>,
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
                            // warn!("Reconstructed share received from {}: {:?}", shares_from_others.sender_id,share);
                            let key = (share.epoch, share.index);
                            // for secret in share.secrets{
                            //
                            // }
                            let shares_vec = self.shares_to_cumulate
                                .entry(key)
                                .or_insert_with(HashMap::new);
                            for single_share in share.secrets {
                                let scalar_set = shares_vec
                                    .entry(single_share.c)
                                    .or_insert_with(HashSet::new);
                                scalar_set.insert((shares_from_others.sender,single_share.y));
                            }
                            // let is_duplicate = shares_vec.iter().any(|(existing_id, existing_share)| {
                            //     *existing_id == shares_from_others.sender && *existing_share == share
                            // });
                            // if !is_duplicate && !self.reconstructed_epoch_wave.contains(&key) {
                            //     // warn!("share to reconstruct :{:?}",share);
                            //     shares_vec.push((shares_from_others.sender, share));
                            // }
                        }
                        _ => {}
                    }
                }
            }

            let committee = self.committee.read().await;
            let threshold = committee.authorities_fault_tolerance() + 1;

            let mut secrets_to_reconstruct = Vec::new();
            self.certificates_to_reconstruct_buffer.retain(|(digests, epoch, index)| {
                let mut digest_can_be_reconstructed = HashSet::new();
                let mut secret_can_be_reconstructed = Vec::new();
                let key = (*epoch, *index);
                if let Some(shares) = self.shares_to_cumulate.get(&key) {
                    for (c,s) in shares.iter() {
                        if s.len() >= threshold {
                            let s: Vec<(PublicKey,Scalar)> = s.iter().cloned().collect();
                            secret_can_be_reconstructed.push((*c, s));
                            digest_can_be_reconstructed.insert(*c);
                        }
                    }
                    if &digest_can_be_reconstructed == digests{
                        secrets_to_reconstruct.push((*epoch, *index, secret_can_be_reconstructed));
                        self.reconstructed_epoch_wave.push(key);
                        self.shares_to_cumulate.remove(&key);
                        return false;
                    }else {
                        return true;
                    }
                    // if shares.len() >= threshold {
                    //     let cumulated_secrets: Vec<_> = shares.iter()
                    //         .filter_map(|(pk, share)| {
                    //             let digest_in_share: HashSet<_> = share.secrets.iter().map(|ss| ss.c).collect();
                    //             if &digest_in_share != digests {
                    //                 return None;
                    //             }
                    //             let cumulated_secret = share.secrets.iter()
                    //                 .fold(Scalar::ZERO, |acc, ss| acc + ss.y);
                    //             Some((pk.clone(), cumulated_secret))
                    //         })
                    //         .collect();
                    //
                    //     if !cumulated_secrets.is_empty() {
                    //         secrets_to_reconstruct.push((*epoch, *index, cumulated_secrets));
                    //         self.reconstructed_epoch_wave.push(key);
                    //         self.shares_to_cumulate.remove(&key);
                    //         return false;
                    //     }
                    // }
                }
                true
            });

            for (epoch, index, secret_set) in secrets_to_reconstruct {
                let mut cumulated_output = Scalar::ZERO;
                for (_,shares) in secret_set {
                    let mut ids = Vec::new();
                    let mut values = Vec::new();
                    for (pk,value) in shares{
                        let id = committee.get_id(&pk).unwrap();
                        ids.push(id);
                        values.push(value);
                    }
                    cumulated_output += BreezeReconResult::interpolate(&ids, &values);
                }
                let recon_output = BreezeReconResult::new(epoch, index, cumulated_output);
                // let secrets: Vec<Scalar> = secret_set.iter().map(|(_, s)| s.clone()).collect();
                // let pks: Vec<PublicKey> = secret_set.iter().map(|(pk, _)| *pk).collect();
                // let ids: Vec<_> = pks.iter()
                //     .map(|pk| committee.get_id(pk).unwrap())
                //     .collect();

                // let recon_output = BreezeReconResult::new(epoch, index, &ids, &secrets);
                self.breeze_result_sender.send((epoch, index, recon_output.scalar_to_random()))
                    .await
                    .expect("breeze_result_sender error to send");
            }








            // let mut secrets_to_reconstruct = Vec::new();
            //
            // // 使用 retain 来同时遍历和删除
            // let committee = self.committee.read().await;
            // self.certificates_to_reconstruct_buffer.retain(|(digests, epoch, index)| {
            //     let key = (*epoch, *index);
            //     if let Some(shares) = self.shares_to_cumulate.get(&key) {
            //         if shares.len() >= committee.authorities_fault_tolerance() + 1 {
            //             let mut cumulated_secrets = Vec::new();
            //             for (pk,share) in shares {
            //                 let mut cumulated_secret = Scalar::ZERO;
            //                 let digest_in_share: HashSet<_> = share.secrets.iter().map(|ss| ss.c).collect();
            //                 if &digest_in_share != digests {
            //                     return true;
            //                 }
            //                 for ss in share.secrets.iter() {
            //                     cumulated_secret += ss.y;
            //                 }
            //                 cumulated_secrets.push((pk.clone(),cumulated_secret));
            //             }
            //
            //             secrets_to_reconstruct.push((*epoch, *index, cumulated_secrets));
            //             self.reconstructed_epoch_wave.push(key);
            //             self.shares_to_cumulate.remove(&key);
            //             return false; // 从 dealers_to_reconstruct_buffer 中删除
            //         }
            //     }
            //     true // 保留在 dealers_to_reconstruct_buffer 中
            // });
            // for (epoch,index,secret_set) in secrets_to_reconstruct{
            //     let secrets: Vec<Scalar> = secret_set.iter().map(|(_,recon_share)| recon_share.clone()).collect();
            //     let pks: Vec<PublicKey> = secret_set.iter().map(|rs| rs.0).collect();
            //     let mut ids = Vec::new();
            //     for pk in pks {
            //         ids.push(committee.get_id(&pk).unwrap());
            //     }
            //     // warn!("ready to gen breezereconresult. secrets: {:?} , ids:{:?}", secrets, ids);
            //     let recon_output = BreezeReconResult::new(epoch, index, &ids, &secrets);
            //     // warn!("recon_output: {:?}", recon_output);
            //     self.breeze_result_sender.send(recon_output).await.expect("breeze_result_sender error to send");
            // }




        }
    }
}
