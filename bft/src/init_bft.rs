use async_trait::async_trait;
use bytes::Bytes;
use config::{Committee, KeyPair};
use crypto::{Digest, PublicKey, SecretKey, Signature};
use futures::SinkExt;
use log::{debug};
use model::bft_message::{DumboContent, DumboMessage};
use model::breeze_structs::{BreezeCertificate};
use network::{MessageHandler, Receiver as NetworkReceiver, ReliableSender, Writer};
use sha2::{Digest as ShaDigest, Sha256};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::net::SocketAddr;
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub struct InitBFT {
    pk: PublicKey,
    sk: SecretKey,
    committee: Committee,
    cer_from_other_receiver: Receiver<DumboMessage>,
    vote_from_other_receiver: Receiver<DumboMessage>,
    decided_from_other_receiver: Receiver<DumboMessage>,

    cer_to_init_consensus_receiver: Receiver<BreezeCertificate>,
    init_cc_to_coord_sender: Sender<Vec<BreezeCertificate>>,
    network: ReliableSender,
}
const CHANNEL_CAPACITY: usize = 1_000;
impl InitBFT {
    pub async fn spawn(
        key_pair: KeyPair,
        address: SocketAddr,
        committee: Committee,
        cer_to_init_consensus_receiver: Receiver<BreezeCertificate>,
        init_cc_to_coord_sender: Sender<Vec<BreezeCertificate>>,
    ) {
        let quorum_threshold = committee.authorities_quorum_threshold();
        let fault_tolerance = committee.authorities_fault_tolerance();

        let (cer_from_other_sender, cer_from_other_receiver) = channel(CHANNEL_CAPACITY);
        let (vote_from_other_sender, vote_from_other_receiver) = channel(CHANNEL_CAPACITY);
        let (decided_from_other_sender, decided_from_other_receiver) = channel(CHANNEL_CAPACITY);
        let pk = key_pair.name;
        let sk = key_pair.secret;
        NetworkReceiver::spawn(
            address,
            InitBFTMessageHandler {
                cer_from_other_sender,
                vote_from_other_sender,
                decided_from_other_sender
            },
        );
        tokio::spawn(async move {
            Self {
                pk,
                sk,
                committee,
                cer_from_other_receiver,
                vote_from_other_receiver,
                decided_from_other_receiver,
                cer_to_init_consensus_receiver,
                init_cc_to_coord_sender,
                network: ReliableSender::new(),
            }
            .run(quorum_threshold, fault_tolerance)
            .await;
        });
    }
    async fn run(
        &mut self,
        quorum_threshold: usize,
        fault_tolerance: usize,
    ) {
        let mut certificate_buffer = HashSet::new();
        let mut cc_buffer = HashMap::new();
        let mut init_cc_decided = false;
        // let mut my_propose = HashSet::new();
        // let mut sig_buffer = HashSet::new();
        loop {
            tokio::select! {
                Some(cer) = self.cer_to_init_consensus_receiver.recv() => {
                    let message = DumboMessage{
                        sender: self.pk,
                        content: DumboContent::Certificate(cer)
                    };
                    self.broadcaster(message).await;
                }
                Some(message) = self.cer_from_other_receiver.recv() =>{
                    match message.content {
                        DumboContent::Certificate(cert) => {
                            if cert.verify(quorum_threshold) {
                                certificate_buffer.insert((message.sender,cert));
                            }
                        }
                        _ => {}
                    }

                }
                // Some(message) = self.cc_from_other_receiver.recv() =>{
                //     let mut flag = true;
                //     match message.content {
                //         DumboContent::Propose(ref cc) => {
                //             for cer in cc.iter(){
                //                 if !cer.verify(quorum_threshold) {
                //                     flag = false;
                //                     break;
                //                 }
                //             }
                //             if !flag{continue}
                //             cc_buffer.insert(message.sender, cc.clone());
                //         },
                //         _ => {}
                //     }
                // }
                Some(message) = self.vote_from_other_receiver.recv() =>{
                    let mut flag = true;
                    match message.content {
                        DumboContent::Vote(ref cc) => {
                            for cer in cc.0.iter(){
                                if !cer.verify(quorum_threshold) {
                                    flag = false;
                                    break;
                                }
                            }
                            let digest = Digest(Self::hash_breeze_certificates(&cc.0));
                            if let Err(_) = cc.1.verify(&digest,&message.sender) { flag = false; }

                            if !flag{continue}
                            cc_buffer.insert(message.sender, cc.clone());
                        },
                        _ => {}
                    }
                }
                Some(message) = self.decided_from_other_receiver.recv() =>{
                    let mut flag = true;
                    match message.content {
                        DumboContent::Decided((cc,sigs)) => {
                            for cer in cc.iter(){
                                if !cer.verify(quorum_threshold) {
                                    flag = false;
                                    break;
                                }
                            }
                            let digest = Digest(Self::hash_breeze_certificates(&cc));
                            for sig in sigs{
                                if let Err(_) = sig.1.verify(&digest,&sig.0) { flag = false; break;}
                            }

                            if !flag{continue}

                            if !init_cc_decided{
                                let res_to_send: Vec<_> = cc.iter().cloned().collect();
                                self.init_cc_to_coord_sender
                                    .send(res_to_send)
                                    .await
                                    .expect("fail to send common core to consensus");
                            }
                            init_cc_decided = true;
                        },
                        _ => {}
                    }
                }
            }

            if certificate_buffer.len() >= fault_tolerance + 1 {
                let cc_to_propose: HashSet<(PublicKey, BreezeCertificate)> = certificate_buffer
                    .drain()
                    .take(fault_tolerance + 1)
                    .collect();
                let my_cc: HashSet<_> = cc_to_propose.iter().map(|x| x.1.clone()).collect();
                let digest = Digest(Self::hash_breeze_certificates(&my_cc));
                let sig = Signature::new(&digest, &self.sk);
                let message = DumboMessage {
                    sender: self.pk,
                    content: DumboContent::Vote((my_cc,sig)),
                };
                self.broadcaster(message).await;
            }
            if cc_buffer.len() >= quorum_threshold {
                match Self::find_result(&cc_buffer) {
                    Some(res) => {
                        let res_to_send: Vec<_> = res.0.iter().cloned().collect();
                        if !init_cc_decided{
                            self.init_cc_to_coord_sender
                                .send(res_to_send)
                                .await
                                .expect("fail to send common core to consensus");
                        }
                        let temp:HashSet<_> = res.0.iter().cloned().collect();
                        let sigs = cc_buffer.iter().map(|x| (x.0.clone(),x.1.1.clone())).collect();
                        let decided = (temp, sigs);
                        let message = DumboMessage {
                            sender: self.pk,
                            content: DumboContent::Decided(decided),
                        };
                        self.broadcaster(message).await;
                        init_cc_decided = true;
                    }
                    None => {
                        let vote = self.find_vote_subset(&cc_buffer, fault_tolerance);
                        let digest = Digest(Self::hash_breeze_certificates(&vote));
                        let sig = Signature::new(&digest, &self.sk);
                        cc_buffer.retain(|_key, (cert_set, _sig)| cert_set == &vote);
                        let message = DumboMessage {
                            sender: self.pk,
                            content: DumboContent::Vote((vote,sig)),
                        };
                        self.broadcaster(message).await;
                    }
                }
                // let ccs: Vec<(PublicKey, HashSet<BreezeCertificate>)> = cc_buffer
                //     .drain()
                //     .take(quorum_threshold)
                //     .collect();


                // let mut sigs_map_to_addresses: HashMap<SocketAddr, Bytes> = HashMap::new();
                // for (owner,cc) in ccs {
                //
                //     let digest = Digest(Self::hash_breeze_certificates(&cc));
                //     let sig = Signature::new(&digest,&self.sk);
                //     let message = DumboMessage{
                //         sender: self.pk,
                //         content: DumboContent::Signature(sig)
                //     };
                //     let bytes = bincode::serialize(&message).expect("Failed to serialize shares in BreezeShare");
                //     let address = Self::find_address(&addresses, &owner).unwrap();
                //     sigs_map_to_addresses.insert(address, Bytes::from(bytes));
                // }
                // let handlers = self.network.dispatch_to_addresses(sigs_map_to_addresses).await;
                // for h in handlers {
                //     if let Err(_e) = h.await {
                //         error!("Dispatching of shares was not successful")
                //     }
                // }
            }
        }
    }

    async fn broadcaster<T: serde::ser::Serialize>(&mut self, message: T) {
        let addresses = self
            .committee
            .all_init_bft_addresses()
            .iter()
            .map(|x| x.1)
            .collect::<Vec<_>>();
        let bytes = bincode::serialize(&message)
            .expect("Failed to serialize shares for reconstruction in BreezeReconstruct");
        let handlers = self.network.broadcast(addresses, Bytes::from(bytes)).await;
        for h in handlers {
            if let Err(_e) = h.await {
                debug!("Broadcast of shares for reconstruction was not successful")
            }
        }
    }

    fn hash_breeze_certificates(certs: &HashSet<BreezeCertificate>) -> [u8; 32] {
        let mut hasher = Sha256::new();
        let serialized = bincode::serialize(&certs).expect("Failed to serialize certificates");
        hasher.update(&serialized);
        let result = hasher.finalize();
        result.into()
    }
    // fn find_address(
    //     addresses: &Vec<(PublicKey, SocketAddr)>,
    //     target_pk: &PublicKey,
    // ) -> Option<SocketAddr> {
    //     addresses
    //         .iter()
    //         .find(|(pk, _)| pk == target_pk)
    //         .map(|(_, addr)| *addr)
    // }

    fn find_result(
        map: &HashMap<PublicKey, (HashSet<BreezeCertificate>,Signature)>,
    ) -> Option<(HashSet<BreezeCertificate>,Signature)> {
        let first_set = map.values().next().unwrap().clone();
        let all_same = map.values().all(|set| set.0 == first_set.0);
        if all_same {
            let result = first_set;
            Some(result)
        } else {
            None
        }
    }
    fn find_vote_subset(
        &self,
        ccs: &HashMap<PublicKey, (HashSet<BreezeCertificate>,Signature)>,
        fault_tolerance: usize,
    ) -> HashSet<BreezeCertificate> {
        let target_size = fault_tolerance + 1;
        let mut certificate_counts: Vec<(HashSet<BreezeCertificate>, usize)> = Vec::new();

        for (_, cert_set) in ccs.iter() {
            if let Some((_existing_set, count)) = certificate_counts
                .iter_mut()
                .find(|(set, _)| set == &cert_set.0)
            {
                *count += 1;
            } else {
                certificate_counts.push((cert_set.0.clone(), 1));
            }
        }

        if let Some((cert_set, _)) = certificate_counts
            .iter()
            .find(|(_, count)| *count >= target_size)
        {
            cert_set.clone()
        } else {
            let mut lowest_cc = (self.committee.size(), self.pk);
            for (pk, _) in ccs.iter() {
                let id = self.committee.get_id(pk).unwrap();
                if id < lowest_cc.0 {
                    lowest_cc.0 = id;
                    lowest_cc.1 = *pk;
                }
            }
            let res = ccs.iter().find(|x| *x.0 == lowest_cc.1).unwrap().1.clone();
            res.0
        }
    }
}
#[derive(Clone)]
pub struct InitBFTMessageHandler {
    cer_from_other_sender: Sender<DumboMessage>,
    vote_from_other_sender: Sender<DumboMessage>,
    decided_from_other_sender: Sender<DumboMessage>
}
#[async_trait]
impl MessageHandler for InitBFTMessageHandler {
    async fn dispatch(&self, writer: &mut Writer, serialized: Bytes) -> Result<(), Box<dyn Error>> {
        let _ = writer.send(Bytes::from("Ack")).await;

        let message: DumboMessage = bincode::deserialize(&serialized).unwrap();

        match message.content {
            DumboContent::Certificate(_) => {
                self.cer_from_other_sender
                    .send(message)
                    .await
                    .expect("Failed to send secret to breeze validator");
            }
            DumboContent::Vote(_) => {
                self.vote_from_other_sender
                    .send(message)
                    .await
                    .expect("Failed to send secret to breeze validator");
            }
            DumboContent::Decided(_) => {
                self.decided_from_other_sender
                    .send(message)
                    .await
                    .expect("Failed to send secret to breeze validator");
            }
        }
        Ok(())
    }
}
