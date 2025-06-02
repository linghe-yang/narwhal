use std::collections::HashMap;
use bytes::Bytes;
use log::{error, info};
use network::{CancelHandler, ReliableSender};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use config::Committee;
use crypto::{PublicKey, SecretKey, Signature};
use model::breeze_universal::CommonReferenceString;
use model::types_and_const::{Epoch, Id};
use crate::breeze_origin::breeze_share_dealer::Shares;
use crate::breeze_structs::{BreezeContent, BreezeMessage};

pub struct BreezeReply {
    node_id: (PublicKey,Id),
    signing_key: SecretKey,
    committee: Arc<RwLock<Committee>>,
    breeze_share_receiver: Receiver<BreezeMessage>,
    network: ReliableSender,
    my_shares: Arc<RwLock<Vec<BreezeMessage>>>,
    common_reference_string: Arc<RwLock<CommonReferenceString>>,
    cancel_handlers: HashMap<Epoch, Vec<CancelHandler>>,
}

impl BreezeReply {
    pub fn spawn(
        node_id: (PublicKey,Id),
        signing_key: SecretKey,
        committee: Arc<RwLock<Committee>>,
        breeze_share_receiver: Receiver<BreezeMessage>,
        network: ReliableSender,
        my_shares: Arc<RwLock<Vec<BreezeMessage>>>,
        common_reference_string: Arc<RwLock<CommonReferenceString>>,
    ) {
        tokio::spawn(async move {
            Self {
                node_id,
                signing_key,
                committee,
                breeze_share_receiver,
                network,
                my_shares,
                common_reference_string,
                cancel_handlers: HashMap::new(),
            }
            .run()
            .await;
        });
    }

    
    pub async fn run(&mut self) {
        info!("Breeze reply start to listen");
        loop {
            match self.breeze_share_receiver.recv().await.unwrap() {
                message => {
                    let my_share = match message.content {
                        BreezeContent::Share(ref share) => share.clone(),
                        _ => {
                            continue;
                        }
                    };
                    
                    let crs = self.common_reference_string.read().await;
                    let committee = self.committee.read().await;
                    if !Shares::verify(
                        &crs,
                        self.node_id.1,
                        committee.authorities_fault_tolerance(),
                        my_share.clone(),
                    ) {
                        continue;
                    }
                    let dealer = message.sender;

                    let signature = Signature::new(&my_share.c, &self.signing_key);
                    
                    let epoch = my_share.epoch;

                    {
                        let mut my_shares = self.my_shares.write().await;
                        let has_duplicate = my_shares
                            .iter()
                            .filter(|msg| msg.sender == message.sender)
                            .any(|msg| match &msg.content {
                                BreezeContent::Share(existing_share) => {
                                    existing_share.epoch == epoch
                                }
                                _ => false,
                            });


                        if has_duplicate {
                            error!("Duplicate message content found for sender_id {}, skipping insertion", dealer);
                            continue;
                        }
                        my_shares.push(message);
                    }

                    let reply = BreezeMessage::new_reply_message(dealer, self.node_id.0, my_share.c, signature, epoch);
                    let bytes = bincode::serialize(&reply)
                        .expect("Failed to serialize reply in BreezeReply");
                    let address = self
                        .committee
                        .read()
                        .await
                        .breeze_address(&dealer)
                        .unwrap();
                    let handler = self.network.send(address, Bytes::from(bytes)).await;

                    self.cancel_handlers
                        .entry(epoch)
                        .or_insert_with(Vec::new)
                        .push(handler);
                }
            }
        }
    }
}
