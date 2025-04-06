use log::{ error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use config::Committee;
use crypto::{Digest, PublicKey};
use model::breeze_structs::{BreezeCertificate, BreezeContent, BreezeMessage};
use model::scale_type::{Epoch, Id};

pub struct BreezeConfirm {
    node_id: (PublicKey,Id),
    committee: Arc<RwLock<Committee>>,
    breeze_confirm_receiver: Receiver<BreezeMessage>,
    breeze_certificate_sender: Sender<BreezeCertificate>,
    my_dealer_shares: Arc<RwLock<HashMap<Epoch,Digest>>>,
}

impl BreezeConfirm {
    pub fn spawn(
        node_id: (PublicKey,Id),
        committee: Arc<RwLock<Committee>>,
        breeze_confirm_receiver: Receiver<BreezeMessage>,
        breeze_certificate_sender: Sender<BreezeCertificate>,
        my_dealer_shares: Arc<RwLock<HashMap<Epoch,Digest>>>,
    ) {

        tokio::spawn(async move {
            Self {
                node_id,
                committee,
                breeze_confirm_receiver,
                breeze_certificate_sender,
                my_dealer_shares
            }
            .run()
            .await;
        });
    }

    pub async fn run(&mut self) {
        info!("Breeze confirm start to listen");
        let mut certificates: HashMap<Epoch, BreezeCertificate> = HashMap::new();
        let mut delivered_certificates: Vec<Epoch> = Vec::new();
        loop {
            match self.breeze_confirm_receiver.recv().await.unwrap() {
                message => {
                    
                    let epoch = match message.get_epoch() {
                        Some(epoch) => epoch,
                        None => {
                            continue;
                        }
                    };
                    let signature;
                    let receiver = message.sender;
                    if let BreezeContent::Reply(rm) = message.content {
                        if rm.dealer != self.node_id.0 {
                            continue;
                        }
                        signature = rm.signature;
                    } else {
                        continue;
                    }
                    let committee = self.committee.read().await;
                    let my_dealer_shares = self.my_dealer_shares.read().await;
                    match my_dealer_shares.get(&epoch) {
                        Some(c) => {
                            if signature.verify(c, &receiver).is_ok() {
                                certificates
                                    .entry(epoch)
                                    .and_modify(|cert| cert.insert(receiver, signature.clone()))
                                    .or_insert(BreezeCertificate::new(*c, receiver,epoch, signature));

                                // let mut keys_to_remove = Vec::new();
                                let quorum_threshold = committee.authorities_quorum_threshold();
                                match certificates.get(&epoch){
                                    Some(cert) => {
                                        if cert.get_len() >= quorum_threshold
                                            && !delivered_certificates.contains(&epoch)
                                        {
                                            if let Err(_) = self.breeze_certificate_sender.send(cert.clone()).await {
                                                error!("fail to send certificate to BFT-SMR")
                                            }
                                            let mut my_dealer_shares_write = self.my_dealer_shares.write().await;
                                            my_dealer_shares_write.remove(&epoch);
                                            // keys_to_remove.push(e);
                                            delivered_certificates.push(epoch.clone());
                                            certificates.retain(|&e, _| e > epoch);
                                        }
                                    }
                                    _ => {continue;}
                                }
                                
                                // for (&e, cert) in certificates.iter_mut() {
                                //     if cert.get_len() >= quorum_threshold
                                //         && !delivered_certificates.contains(&e)
                                //     {
                                //         if let Err(_) = self.breeze_certificate_sender.send(cert.clone()).await {
                                //             error!("fail to send certificate to BFT-SMR")
                                //         }
                                //         let mut my_dealer_shares_write = self.my_dealer_shares.write().await;
                                //         my_dealer_shares_write.remove(&epoch);
                                //         keys_to_remove.push(e);
                                //         delivered_certificates.push(e.clone());
                                //     }
                                // }
                                // for key in keys_to_remove {
                                //     certificates.retain(|&e, _| e > key);
                                // }
                            }
                        }
                        None => {}
                    }
                    drop(my_dealer_shares);
                }
            }
        }
    }
}
