use std::collections::HashSet;
use bytes::Bytes;
use log::{error, info};
use network::ReliableSender;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use config::Committee;
use crypto::{Digest, PublicKey};
use model::breeze_structs::{BreezeContent, BreezeMessage, BreezeReconRequest, ReconstructShare, SingleShare};
use model::scale_type::{Epoch, Id};

pub struct BreezeReconstruct {
    node_id: (PublicKey,Id),
    committee: Arc<RwLock<Committee>>,
    breeze_reconstruct_cmd_receiver: Receiver<BreezeReconRequest>,
    breeze_recon_certificate_sender: Sender<(HashSet<Digest>,Epoch, usize)>,
    network: ReliableSender,
    my_shares: Arc<RwLock<Vec<BreezeMessage>>>,
}

impl BreezeReconstruct {
    pub fn spawn(
        node_id: (PublicKey,Id),
        committee: Arc<RwLock<Committee>>,
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
                    //是否需要await？
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
                                    // let wave_share = Share{
                                    //     c:share.c,
                                    //     r_hat: vec![share.r_hat[index]],
                                    //     r_witness: vec![share.r_witness[index].clone()],
                                    //     y_k: vec![share.y_k[index]],
                                    //     phi_k: PhiElement{
                                    //         IProof: vec![share.phi_k.IProof[index].clone()],
                                    //         D_hat: share.phi_k.D_hat,
                                    //         V_D_i: share.phi_k.V_D_i
                                    //     },
                                    //     n: share.n,
                                    //     epoch: share.epoch,
                                    // };
                                    let single_share = SingleShare{
                                        c:share.c,
                                        y: share.y_k[index]
                                    };
                                    my_secrets_to_broadcast.push(single_share);

                                }
                            }
                            // if bm.c == id_to_cumulate {
                            //     if let BreezeContent::Share(share) = &bm.content {
                            //         if share.epoch == message.1
                            //             && message.2 <= share.y_k.len() as u64
                            //         {
                            //             let point: Scalar =
                            //                 share.y_k[message.2 as usize - 1].clone();
                            //             cumulated_secret += point;
                            //         }
                            //     }
                            // }
                        }
                    }
                    let reconstruct_message = BreezeMessage::new_reconstruct_message(
                        self.node_id.0,
                        ReconstructShare::new(my_secrets_to_broadcast, message.epoch, message.index),
                    );
                    let addresses = self.committee.read().await.all_breeze_addresses().iter().map(|a| a.1).collect::<Vec<_>>();

                    let bytes = bincode::serialize(&reconstruct_message).expect(
                        "Failed to serialize shares for reconstruction in BreezeReconstruct",
                    );
                    let handlers = self.network.broadcast(addresses, Bytes::from(bytes)).await;
                    for h in handlers {
                        if let Err(_e) = h.await {
                            error!("Broadcast of shares for reconstruction was not successful")
                        }
                    }
                }
            }
        }
    }
}
