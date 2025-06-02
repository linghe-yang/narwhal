use std::collections::{HashMap, HashSet};
use bytes::Bytes;
use log::{info};
use network::{CancelHandler, ReliableSender};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use config::Committee;
use crypto::{Digest, PublicKey};
use model::breeze_universal::BreezeReconRequest;
use model::types_and_const::{Epoch, Id};
use crate::breeze_structs::{BreezeContent, BreezeMessage, ReconstructShare, SingleShare};

pub struct BreezeReconstruct {
    node_id: (PublicKey,Id),
    committee: Committee,
    breeze_reconstruct_cmd_receiver: Receiver<BreezeReconRequest>,
    breeze_recon_certificate_sender: Sender<(HashSet<Digest>,Epoch, usize)>,
    network: ReliableSender,
    my_shares: Arc<RwLock<Vec<BreezeMessage>>>,
    cancel_handlers: HashMap<(Epoch, usize), Vec<CancelHandler>>,
}

impl BreezeReconstruct {
    pub fn spawn(
        node_id: (PublicKey,Id),
        committee: Committee,
        breeze_reconstruct_cmd_receiver: Receiver<BreezeReconRequest>,
        breeze_recon_certificate_sender: Sender<(HashSet<Digest>,Epoch, usize)>,
        network: ReliableSender,
        my_shares: Arc<RwLock<Vec<BreezeMessage>>>,
    ) {
        tokio::spawn(async move {
            Self {
                node_id,
                committee,
                breeze_reconstruct_cmd_receiver,
                breeze_recon_certificate_sender,
                network,
                my_shares,
                cancel_handlers: HashMap::new(),
            }
            .run()
            .await;
        });
    }

    pub async fn run(&mut self) {
        info!("Breeze reconstruct start to listen");
        loop {
            match self.breeze_reconstruct_cmd_receiver.recv().await.unwrap() {
                message => {
                    self.breeze_recon_certificate_sender
                        .send((message.c.clone(), message.epoch, message.index))
                        .await
                        .unwrap();
                    let shares = self.my_shares.read().await; // Get read lock on my_shares

                    let mut my_secrets_to_broadcast = Vec::new();
                    for c in message.c {
                        for bm in shares.iter() {
                            if let BreezeContent::Share(share) = &bm.content {
                                if share.epoch == message.epoch && share.c == c
                                    && message.index <= share.y_k.len()
                                {
                                    let index = message.index - 1;
                                    let single_share = SingleShare{
                                        c:share.c,
                                        y: share.y_k[index]
                                    };
                                    my_secrets_to_broadcast.push(single_share);

                                }
                            }
                        }
                    }
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
