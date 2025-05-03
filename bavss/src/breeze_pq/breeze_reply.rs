use crate::breeze_pq::breeze_share_dealer::Shares;
use crate::breeze_structs::{BreezeContent, BreezeMessage, PQCrs, Share};
use bytes::Bytes;
use config::Committee;
use crypto::{Digest, PublicKey, SecretKey, Signature};
use log::info;
use model::types_and_const::{Epoch, Id};
use network::{CancelHandler, ReliableSender};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{RwLock};

pub struct BreezeReply {
    node_id: (PublicKey, Id),
    signing_key: SecretKey,
    committee: Committee,
    breeze_share_receiver: Receiver<BreezeMessage>,
    breeze_merkle_roots_receiver: Receiver<BreezeMessage>,
    merkle_roots_received: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Vec<Digest>>>>>,
    merkle_watch_sender:Sender<Epoch>,
    shares_received: HashMap<Epoch, HashMap<PublicKey, Share>>,
    network: ReliableSender,
    valid_shares: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Share>>>>,
    common_reference_string: Arc<PQCrs>,
    cancel_handlers: HashMap<Epoch, Vec<CancelHandler>>,
}

impl BreezeReply {
    pub fn spawn(
        node_id: (PublicKey, Id),
        signing_key: SecretKey,
        // committee: Arc<RwLock<Committee>>,
        committee: Committee,
        breeze_share_receiver: Receiver<BreezeMessage>,
        breeze_merkle_roots_receiver: Receiver<BreezeMessage>,
        merkle_roots_received: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Vec<Digest>>>>>,
        merkle_watch_sender:Sender<Epoch>,
        network: ReliableSender,
        valid_shares: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Share>>>>,
        common_reference_string: Arc<PQCrs>,
    ) {
        tokio::spawn(async move {
            Self {
                node_id,
                signing_key,
                committee,
                breeze_share_receiver,
                breeze_merkle_roots_receiver,
                merkle_roots_received,
                merkle_watch_sender,
                shares_received: HashMap::new(),
                network,
                valid_shares,
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
            // let committee = self.committee.read().await;
            tokio::select! {
                Some(message) = self.breeze_share_receiver.recv() => {
                    let my_share = match message.content {
                        BreezeContent::Share(ref share) => share.clone(),
                        _ => {
                            continue;
                        }
                    };

                    if message.sender != self.node_id.0 {
                        if !Shares::verify_shares(
                            &self.common_reference_string,
                            &my_share,
                            self.node_id.1,
                        ) {
                            continue;
                        }
                    }

                    let inner_map = self.shares_received.entry(my_share.epoch).or_insert_with(HashMap::new);
                    inner_map.entry(message.sender).or_insert(my_share);
                },
                Some(message) = self.breeze_merkle_roots_receiver.recv() => {
                    match message.content {
                        BreezeContent::Merkle(mr) =>{
                            let mut merkle_roots = self.merkle_roots_received.write().await;
                            let inner_map = merkle_roots
                                .entry(mr.epoch)
                                .or_insert_with(HashMap::new);
                            inner_map.entry(message.sender).or_insert(mr.roots);
                            drop(merkle_roots);
                            self.merkle_watch_sender.send(mr.epoch).await.unwrap();
                        }
                        _ => continue,
                    }
                }
            }
            let mut reply_msgs = Vec::new();
            let merkle_roots = self.merkle_roots_received.read().await;
            for (epoch, share_map) in &self.shares_received {
                // 检查 merkle_roots_received 中是否有相同的 Epoch
                if let Some(merkle_map) = merkle_roots.get(epoch) {
                    // 遍历 share_map 的 PublicKey
                    for (pk, share) in share_map {
                        // 检查 merkle_map 中是否有相同的 PublicKey
                        if let Some(digests) = merkle_map.get(pk) {
                            if *pk == self.node_id.0 {
                                let signature = Signature::new(&share.c, &self.signing_key);
                                reply_msgs.push((*pk, share, signature, *epoch));
                            }
                            else if Shares::verify_merkle_batch(self.node_id.1,share, digests) {
                                let signature = Signature::new(&share.c, &self.signing_key);
                                reply_msgs.push((*pk, share, signature, *epoch));
                            }
                        }
                    }
                }
            }
            let mut valid_shares = self.valid_shares.write().await;
            for (dealer_pk, share, sig, epoch) in reply_msgs {
                let reply =
                    BreezeMessage::new_reply_message(dealer_pk, self.node_id.0, share.c, sig, epoch);
                let bytes =
                    bincode::serialize(&reply).expect("Failed to serialize reply in BreezeReply");
                let address = self.committee.breeze_address(&dealer_pk).unwrap();
                let handler = self.network.send(address, Bytes::from(bytes)).await;
                self.cancel_handlers
                    .entry(epoch)
                    .or_insert_with(Vec::new)
                    .push(handler);

                let inner_map =
                    valid_shares.entry(epoch).or_insert_with(HashMap::new);
                inner_map.entry(dealer_pk).or_insert(share.clone());
            }
        }
    }
}
