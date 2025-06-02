use std::collections::{HashMap, HashSet};
use bytes::Bytes;
use log::{info};
use network::{CancelHandler, ReliableSender};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use config::Committee;
use crypto::{Digest, PublicKey};
use model::breeze_universal::{BreezeReconRequest};
use model::types_and_const::{Epoch, Id};
use crate::breeze_structs::{ BreezeMessage, PQCrs, ReconstructShare, Share, SingleShare};

pub struct BreezeReconstruct {
    node_id: (PublicKey,Id),
    committee: Committee,
    breeze_reconstruct_cmd_receiver: Receiver<BreezeReconRequest>,
    breeze_recon_certificate_sender: Sender<(HashSet<Digest>,Epoch, usize)>,
    network: ReliableSender,
    valid_shares: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Share>>>>,
    cancel_handlers: HashMap<(Epoch, usize), Vec<CancelHandler>>,
    common_reference_string: Arc<PQCrs>,
}

impl BreezeReconstruct {
    pub fn spawn(
        node_id: (PublicKey,Id),
        committee: Committee,
        breeze_reconstruct_cmd_receiver: Receiver<BreezeReconRequest>,
        breeze_recon_certificate_sender: Sender<(HashSet<Digest>,Epoch, usize)>,
        network: ReliableSender,
        valid_shares: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Share>>>>,
        common_reference_string: Arc<PQCrs>,
    ) {
        tokio::spawn(async move {
            Self {
                node_id,
                committee,
                breeze_reconstruct_cmd_receiver,
                breeze_recon_certificate_sender,
                network,
                valid_shares,
                cancel_handlers: HashMap::new(),
                common_reference_string
            }
            .run()
            .await;
        });
    }

    pub async fn run(&mut self) {
        info!("Breeze reconstruct start to listen");
        let g = self.common_reference_string.g;
        loop {
            match self.breeze_reconstruct_cmd_receiver.recv().await.unwrap() {
                message => {
                    let idx = (message.index - 1) * g;
                    self.breeze_recon_certificate_sender
                        .send((message.c.clone(), message.epoch, message.index))
                        .await
                        .unwrap();
                    let shares_lock = self.valid_shares.read().await;
                    let epoch_shares = match shares_lock.get(&message.epoch) {
                        Some(shares) => shares,
                        None => continue,
                    };
                    let my_secrets_to_broadcast: Vec<SingleShare> = epoch_shares
                        .iter()
                        .filter(|(_pk, share)| message.c.contains(&share.c))
                        .map(|(pk, share)| SingleShare {
                            dealer: *pk,
                            c: share.c,
                            y: share.y_k[idx..idx+g].to_vec(),
                            merkle_proof: (self.node_id.1,share.merkle_proofs[idx..idx+g].to_vec()),
                            total_party_num: share.total_party_num,
                        })
                        .collect();
                    let reconstruct_message = BreezeMessage::new_reconstruct_message(
                        self.node_id.0,
                        ReconstructShare::new(my_secrets_to_broadcast, message.epoch, message.index),
                    );
                    let addresses = self.committee.all_breeze_addresses().iter().map(|a| a.1).collect::<Vec<_>>();
                    let bytes = bincode::serialize(&reconstruct_message).expect(
                        "Failed to serialize shares for reconstruction in BreezeReconstruct",
                    );
                    let handlers = self.network.broadcast(addresses, Bytes::from(bytes)).await;
                    self.cancel_handlers
                        .entry((message.epoch, message.index))
                        .or_insert_with(Vec::new)
                        .extend(handlers);
                }
            }
        }
    }
}
