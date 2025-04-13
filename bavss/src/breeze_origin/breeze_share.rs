use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use bytes::Bytes;
use log::{info};
use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use config::Committee;
use crypto::{Digest, PublicKey};
use model::breeze_universal::CommonReferenceString;
use model::types_and_const::{Epoch, Id, BEACON_PER_EPOCH, MAX_EPOCH};
use network::{CancelHandler, ReliableSender};
use crate::breeze_origin::breeze_share_dealer::Shares;
use crate::breeze_structs::BreezeMessage;

pub struct BreezeShare{
    node_id: (PublicKey,Id),
    committee: Arc<RwLock<Committee>>,
    breeze_share_cmd_receiver: Receiver<Epoch>,
    network: ReliableSender,
    common_reference_string: Arc<RwLock<CommonReferenceString>>,
    my_dealer_shares: Arc<RwLock<HashMap<Epoch,Digest>>>,
    cancel_handlers: HashMap<Epoch, Vec<CancelHandler>>,
}

impl BreezeShare {
    pub fn spawn(
        node_id: (PublicKey,Id),
        committee: Arc<RwLock<Committee>>,
        breeze_share_cmd_receiver: Receiver<Epoch>,
        network: ReliableSender,
        common_reference_string: Arc<RwLock<CommonReferenceString>>,
        my_dealer_shares: Arc<RwLock<HashMap<Epoch,Digest>>>
    ) {
        tokio::spawn(async move {
            Self {
                node_id,
                committee,
                breeze_share_cmd_receiver,
                network,
                common_reference_string,
                my_dealer_shares,
                cancel_handlers: HashMap::new(),
            }
            .run()
            .await;
        });
    }

    pub async fn run(&mut self) {
        info!("Breeze share start to listen");
        loop {
            match self.breeze_share_cmd_receiver.recv().await.unwrap() {
                epoch => {
                    let committee = self.committee.read().await;
                    let ids = committee.get_all_ids();
                    let fault_tolerance = committee.authorities_fault_tolerance();
                    let crs = self.common_reference_string.read().await;
                    let batch_size = *MAX_EPOCH.get().unwrap() + *BEACON_PER_EPOCH.get().unwrap();
                    let shares = Shares::new(batch_size as usize, epoch, ids, fault_tolerance, &crs);
                    
                    let mut share_map_to_addresses: HashMap<SocketAddr, Bytes> = HashMap::new();
                    let c = shares.get_c_ref();
                    // 遍历 share_map.value 中的每个 (id, shares) 对
                    for (share, pk) in shares.get_shares_ref() {
                        // 通过 id 获取对应的 address
                        if let Ok(address) = self.committee.read().await.breeze_address(&pk) {
                            // 将 shares 插入到新的 HashMap 中，以 address 作为 key
                            let message = BreezeMessage::new_share_message(self.node_id.0, share.clone());
                            let bytes = bincode::serialize(&message).expect("Failed to serialize shares in BreezeShare");
                            share_map_to_addresses.insert(address, Bytes::from(bytes));
                        }
                    }
                    let mut my_dealer_shares = self.my_dealer_shares.write().await;
                    my_dealer_shares.insert(epoch, *c);
                    let handlers = self.network.dispatch_to_addresses(share_map_to_addresses).await;
                    self.cancel_handlers
                        .entry(epoch)
                        .or_insert_with(Vec::new)
                        .extend(handlers);
                    info!("Breeze share successfully completed");
                }
            }
        }
    }
}