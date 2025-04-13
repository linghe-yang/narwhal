use crate::breeze_pq::breeze_reconstruct_dealer::BreezeReconResult;
use crate::breeze_structs::{BreezeContent, BreezeMessage, PQCrs, SingleShare};
use crate::Secret;
use config::Committee;
use crypto::{Digest, PublicKey};
use log::{error, info};
use model::types_and_const::{Epoch, RandomNum, MAX_INDEX};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{watch, RwLock};
use crate::breeze_pq::breeze_share_dealer::Shares;

pub struct BreezeResult {
    // committee: Arc<RwLock<Committee>>,
    committee: Committee,
    // breeze_recon_certificate_receiver: Receiver<(HashSet<Digest>,Epoch, usize)>,
    // breeze_reconstruct_secret_receiver: Receiver<BreezeMessage>,
    breeze_result_sender: Sender<(Epoch, usize, RandomNum)>,
    // merkle_roots_received: Arc<RwLock<HashMap<Epoch,HashMap<PublicKey,Vec<Digest>>>>>,
    // certificates_to_reconstruct_buffer: Vec<(HashSet<Digest>, Epoch, usize)>,
    certificates_to_reconstruct_buffer: Arc<RwLock<Vec<(HashSet<Digest>, Epoch, usize)>>>,
    // shares_unverified_yet: Arc<RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashSet<(PublicKey, SingleShare)>>>>>,
    shares_verified:
        Arc<RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashMap<PublicKey, Vec<Secret>>>>>>,
    // reconstructed_epoch_wave: Vec<(Epoch, usize)>
    reconstructed_epoch_wave: Arc<RwLock<Vec<(Epoch, usize)>>>,

    shares_verified_watch_receiver: watch::Receiver<()>,
    common_reference_string: Arc<PQCrs>,
}

impl BreezeResult {
    pub fn spawn(
        // committee: Arc<RwLock<Committee>>,
        committee: Committee,
        breeze_recon_certificate_receiver: Receiver<(HashSet<Digest>, Epoch, usize)>,
        breeze_reconstruct_secret_receiver: Receiver<BreezeMessage>,
        merkle_roots_received: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Vec<Digest>>>>>,
        merkle_watch_receiver: watch::Receiver<()>,
        breeze_result_sender: Sender<(Epoch, usize, RandomNum)>,
        common_reference_string: Arc<PQCrs>,
    ) {
        let shares_unverified_yet: Arc<
            RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashMap<PublicKey, SingleShare>>>>,
        > = Arc::new(RwLock::new(HashMap::new()));
        let shares_verified: Arc<
            RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashMap<PublicKey, Vec<Secret>>>>>,
        > = Arc::new(RwLock::new(HashMap::new()));
        let reconstructed_epoch_wave: Arc<RwLock<Vec<(Epoch, usize)>>> =
            Arc::new(RwLock::new(Vec::new()));
        let certificates_to_reconstruct_buffer: Arc<RwLock<Vec<(HashSet<Digest>, Epoch, usize)>>> =
            Arc::new(RwLock::new(Vec::new()));

        let (shares_verified_watch_sender, shares_verified_watch_receiver) = watch::channel(());

        let g = common_reference_string.g;
        tokio::spawn(Self::merkle_watch_monitor(
            merkle_watch_receiver,
            Arc::clone(&merkle_roots_received),
            Arc::clone(&shares_unverified_yet),
            Arc::clone(&shares_verified),
            shares_verified_watch_sender.clone(),
            g
        ));
        tokio::spawn(Self::share_monitor(
            breeze_reconstruct_secret_receiver,
            Arc::clone(&merkle_roots_received),
            Arc::clone(&shares_verified),
            Arc::clone(&shares_unverified_yet),
            shares_verified_watch_sender.clone(),
            g
        ));
        tokio::spawn(Self::certificate_monitor(
            breeze_recon_certificate_receiver,
            Arc::clone(&reconstructed_epoch_wave),
            Arc::clone(&certificates_to_reconstruct_buffer)
        ));

        tokio::spawn(async move {
            Self {
                committee,
                // breeze_recon_certificate_receiver,
                // breeze_reconstruct_secret_receiver,
                breeze_result_sender,
                // merkle_roots_received,
                certificates_to_reconstruct_buffer: Arc::clone(&certificates_to_reconstruct_buffer),
                // shares_unverified_yet,
                shares_verified,
                reconstructed_epoch_wave: Arc::clone(&reconstructed_epoch_wave),

                shares_verified_watch_receiver,
                common_reference_string
            }
            .run()
            .await;
        });
    }

    pub async fn run(&mut self) {
        info!("Breeze result start to listen");
        let threshold = self.committee.authorities_fault_tolerance() + 1;
        let q = self.common_reference_string.q;
        loop {
            if self.shares_verified_watch_receiver.changed().await.is_ok() {
                let mut secrets_to_reconstruct = Vec::new();
                let mut certificates_to_reconstruct_buffer =
                    self.certificates_to_reconstruct_buffer.write().await;
                let shares_verified = self.shares_verified.read().await;

                let mut key_changed = Vec::new();
                certificates_to_reconstruct_buffer.retain(|(digests, epoch, index)| {
                    let mut digest_can_be_reconstructed = HashSet::new();
                    let mut secret_can_be_reconstructed = Vec::new();
                    let key = (*epoch, *index);

                    if let Some(shares) = shares_verified.get(&key) {
                        for (c, s) in shares.iter() {
                            if s.len() >= threshold {
                                let s: Vec<_> = s.iter().map(|(pk, s)| (*pk,s.clone())).collect();
                                secret_can_be_reconstructed.push((*c, s));
                                digest_can_be_reconstructed.insert(*c);
                            }
                        }
                        return if &digest_can_be_reconstructed == digests {
                            secrets_to_reconstruct.push((
                                *epoch,
                                *index,
                                secret_can_be_reconstructed,
                            ));
                            key_changed.push(key);
                            false
                        } else {
                            true
                        };
                    }
                    true
                });
                if !key_changed.is_empty() {
                    drop(shares_verified);
                    let mut reconstructed_epoch_wave = self.reconstructed_epoch_wave.write().await;
                    let mut shares_verified = self.shares_verified.write().await;
                    for key in key_changed {
                        reconstructed_epoch_wave.push(key);
                        shares_verified.remove(&key);
                    }
                }

                for (epoch, index, secret_set) in secrets_to_reconstruct {
                    let mut cumulated_output = vec![0;self.common_reference_string.g];
                    for (_, shares) in secret_set {
                        let mut ids = Vec::new();
                        let mut values = Vec::new();
                        for (pk, value) in shares {
                            // let id = committee.get_id(&pk).unwrap();
                            let id = self.committee.get_id(&pk).unwrap();
                            ids.push(id);
                            values.push(value);
                        }
                        BreezeReconResult::interpolate(&ids, &values, q, &mut cumulated_output);
                    }
                    let recon_output = BreezeReconResult::new(cumulated_output);
                    self.breeze_result_sender
                        .send((epoch, index, recon_output.secret_to_number()))
                        .await
                        .expect("breeze_result_sender error to send");
                }
            } else {
                break;
            }
            // tokio::select! {
            //     Some(certificates_to_reconstruct) = self.breeze_recon_certificate_receiver.recv() => {
            //         let key = (certificates_to_reconstruct.1, certificates_to_reconstruct.2);
            //         if !self.reconstructed_epoch_wave.contains(&key) {
            //             let exists = self.certificates_to_reconstruct_buffer.iter().any(|(_, e, w)| e == &certificates_to_reconstruct.1 && w == &certificates_to_reconstruct.2);
            //             if !exists {
            //                 self.certificates_to_reconstruct_buffer.push(certificates_to_reconstruct);
            //             }
            //         }
            //     },
            //     Some(shares_from_others) = self.breeze_reconstruct_secret_receiver.recv() => {
            //         match shares_from_others.content {
            //             BreezeContent::Reconstruct(share) => {
            //                 let key = (share.epoch, share.index);
            //                 let merkle_roots_received = self.merkle_roots_received.read().await;
            //                 match merkle_roots_received.get(&share.epoch) {
            //                     Some(roots) => {
            //                         for ss in share.secrets.iter() {
            //                             if roots.contains_key(&ss.dealer){
            //                                 if true {
            //                                     let mut write_lock = self.shares_verified.write().await;
            //                                     let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
            //                                     let temp2 = temp.entry(ss.c).or_insert(HashSet::new());
            //                                     temp2.insert((ss.dealer,ss.y));
            //                                 }
            //                             }else {
            //                                 let mut write_lock = self.shares_unverified_yet.write().await;
            //                                 let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
            //                                 let temp2 = temp.entry(ss.c).or_insert(HashSet::new());
            //                                 temp2.insert((ss.dealer,ss.clone()));
            //                             }
            //                         }
            //
            //                     }
            //                     None => {
            //                         for ss in share.secrets.iter() {
            //                             let mut write_lock = self.shares_unverified_yet.write().await;
            //                                 let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
            //                                 let temp2 = temp.entry(ss.c).or_insert(HashSet::new());
            //                                 temp2.insert((ss.dealer,ss.clone()));
            //                         }
            //                     }
            //                 }
            //             }
            //             _ => {}
            //         }
            //     }
            // }

            // let committee = self.committee.read().await;
            // let threshold = committee.authorities_fault_tolerance() + 1;

            // let mut secrets_to_reconstruct = Vec::new();
            // self.certificates_to_reconstruct_buffer.retain(|(digests, epoch, index)| {
            //     let mut digest_can_be_reconstructed = HashSet::new();
            //     let mut secret_can_be_reconstructed = Vec::new();
            //     let key = (*epoch, *index);
            //     if let Some(shares) = self.shares_verified.get(&key) {
            //         for (c,s) in shares.iter() {
            //             if s.len() >= threshold {
            //                 let s: Vec<(PublicKey,Secret)> = s.iter().cloned().collect();
            //                 secret_can_be_reconstructed.push((*c, s));
            //                 digest_can_be_reconstructed.insert(*c);
            //             }
            //         }
            //         return if &digest_can_be_reconstructed == digests {
            //             secrets_to_reconstruct.push((*epoch, *index, secret_can_be_reconstructed));
            //             self.reconstructed_epoch_wave.push(key);
            //             self.shares_verified.remove(&key);
            //             false
            //         } else {
            //             true
            //         }
            //     }
            //     true
            // });
            //
            // for (epoch, index, secret_set) in secrets_to_reconstruct {
            //     let mut cumulated_output = 0;
            //     for (_,shares) in secret_set {
            //         let mut ids = Vec::new();
            //         let mut values = Vec::new();
            //         for (pk,value) in shares{
            //             // let id = committee.get_id(&pk).unwrap();
            //             let id = self.committee.get_id(&pk).unwrap();
            //             ids.push(id);
            //             values.push(value);
            //         }
            //         cumulated_output += BreezeReconResult::interpolate(&ids, &values);
            //     }
            //     let recon_output = BreezeReconResult::new(cumulated_output);
            //     self.breeze_result_sender.send((epoch, index, recon_output.secret_to_number()))
            //         .await
            //         .expect("breeze_result_sender error to send");
            // }
        }
    }

    async fn merkle_watch_monitor(
        mut merkle_watch_receiver: watch::Receiver<()>,
        merkle_roots_received: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Vec<Digest>>>>>,
        shares_unverified_yet: Arc<
            RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashMap<PublicKey, SingleShare>>>>,
        >,
        shares_verified: Arc<
            RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashMap<PublicKey, Vec<Secret>>>>>,
        >,

        shares_verified_watch_sender: watch::Sender<()>,
        g: usize
    ) {
        loop {
            if merkle_watch_receiver.changed().await.is_ok() {
                let merkle_roots_received = merkle_roots_received.read().await;
                let mut shares_unverified_yet = shares_unverified_yet.write().await;
                for ((epoch, index), secrets) in shares_unverified_yet.iter_mut() {
                    match merkle_roots_received.get(&epoch) {
                        Some(roots) => {
                            for (digest, set) in secrets.iter() {
                                for (receiver_pk,ss) in set.iter() {
                                    if roots.contains_key(&ss.dealer) {
                                        let rs = roots.get(&ss.dealer).unwrap();
                                        let idx = (index - 1) * g;
                                        if Shares::verify_merkle(&ss.y, ss.merkle_proof.clone(), rs[idx..idx + g].to_vec(), ss.total_party_num) {
                                            let mut write_lock = shares_verified.write().await;
                                            let temp = write_lock
                                                .entry((*epoch, *index))
                                                .or_insert(HashMap::new());
                                            let temp2 =
                                                temp.entry(*digest).or_insert(HashMap::new());
                                            temp2.insert(*receiver_pk, ss.y.clone());

                                            shares_verified_watch_sender.send(()).unwrap();
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                error!("merkle roots receiver notifier closed, exiting monitor");
                break;
            }
        }
    }

    async fn share_monitor(
        mut breeze_reconstruct_secret_receiver: Receiver<BreezeMessage>,
        merkle_roots_received: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Vec<Digest>>>>>,
        shares_verified: Arc<
            RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashMap<PublicKey, Vec<Secret>>>>>,
        >,
        shares_unverified_yet: Arc<
            RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashMap<PublicKey, SingleShare>>>>,
        >,

        shares_verified_watch_sender: watch::Sender<()>,
        g: usize,
    ){
        loop{
            let shares_from_others = breeze_reconstruct_secret_receiver.recv().await.unwrap();
            match shares_from_others.content {
                BreezeContent::Reconstruct(share) => {
                    let max_index = *MAX_INDEX.get().unwrap();
                    if share.index > max_index{
                        continue;
                    }
                    // let key = (share.epoch, share.index);
                    let merkle_roots_received = merkle_roots_received.read().await;
                    match merkle_roots_received.get(&share.epoch) {
                        Some(roots) => {
                            for ss in share.secrets.iter() {
                                if roots.contains_key(&ss.dealer){
                                    let rs = roots.get(&ss.dealer).unwrap();
                                    let idx = (share.index - 1) * g;
                                    if Shares::verify_merkle(&ss.y, ss.merkle_proof.clone(), rs[idx..idx+g].to_vec(), ss.total_party_num) {
                                        let mut write_lock = shares_verified.write().await;
                                        let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
                                        let temp2 = temp.entry(ss.c).or_insert(HashMap::new());
                                        temp2.insert(shares_from_others.sender,ss.y.clone());
                                        drop(write_lock);
                                        shares_verified_watch_sender.send(()).unwrap();
                                    }else {
                                    }
                                }else {
                                    let mut write_lock = shares_unverified_yet.write().await;
                                    let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
                                    let temp2 = temp.entry(ss.c).or_insert(HashMap::new());
                                    temp2.insert(shares_from_others.sender,ss.clone());
                                    drop(write_lock);
                                }
                            }

                        }
                        None => {
                            for ss in share.secrets.iter() {
                                let mut write_lock = shares_unverified_yet.write().await;
                                let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
                                let temp2 = temp.entry(ss.c).or_insert(HashMap::new());
                                temp2.insert(shares_from_others.sender,ss.clone());
                                drop(write_lock);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    async fn certificate_monitor(
        mut breeze_recon_certificate_receiver: Receiver<(HashSet<Digest>, Epoch, usize)>,
        reconstructed_epoch_wave: Arc<RwLock<Vec<(Epoch, usize)>>>,
        certificates_to_reconstruct_buffer: Arc<RwLock<Vec<(HashSet<Digest>, Epoch, usize)>>>,
        // mut breeze_reconstruct_secret_receiver: Receiver<BreezeMessage>,
        // merkle_roots_received: Arc<RwLock<HashMap<Epoch, HashMap<PublicKey, Vec<Digest>>>>>,
        // shares_verified: Arc<
        //     RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashSet<(PublicKey, Secret)>>>>,
        // >,
        // shares_unverified_yet: Arc<
        //     RwLock<HashMap<(Epoch, usize), HashMap<Digest, HashSet<(PublicKey, SingleShare)>>>>,
        // >,
        //
        // shares_verified_watch_sender: watch::Sender<()>,
    ) {
        loop {
            let certificates_to_reconstruct = breeze_recon_certificate_receiver.recv().await.unwrap();
            let key = (certificates_to_reconstruct.1, certificates_to_reconstruct.2);
            if !reconstructed_epoch_wave.read().await.contains(&key) {
                let mut certificates_to_reconstruct_buffer = certificates_to_reconstruct_buffer.write().await;
                let exists = certificates_to_reconstruct_buffer.iter().any(|(_, e, w)| e == &certificates_to_reconstruct.1 && w == &certificates_to_reconstruct.2);
                if !exists {
                    certificates_to_reconstruct_buffer.push(certificates_to_reconstruct);
                }
                drop(certificates_to_reconstruct_buffer);
            }
            // tokio::select! {
            //     Some(certificates_to_reconstruct) = breeze_recon_certificate_receiver.recv() => {
            //         let key = (certificates_to_reconstruct.1, certificates_to_reconstruct.2);
            //         if !reconstructed_epoch_wave.read().await.contains(&key) {
            //             let mut certificates_to_reconstruct_buffer = certificates_to_reconstruct_buffer.write().await;
            //             let exists = certificates_to_reconstruct_buffer.iter().any(|(_, e, w)| e == &certificates_to_reconstruct.1 && w == &certificates_to_reconstruct.2);
            //             if !exists {
            //                 certificates_to_reconstruct_buffer.push(certificates_to_reconstruct);
            //             }
            //         }
            //     },
            //     Some(shares_from_others) = breeze_reconstruct_secret_receiver.recv() => {
            //         info!("breeze result share from others received");
            //         match shares_from_others.content {
            //             BreezeContent::Reconstruct(share) => {
            //                 let max_index = *MAX_INDEX.get().unwrap();
            //                 if share.index > max_index{
            //                     continue;
            //                 }
            //                 // let key = (share.epoch, share.index);
            //                 let merkle_roots_received = merkle_roots_received.read().await;
            //                 match merkle_roots_received.get(&share.epoch) {
            //                     Some(roots) => {
            //                         for ss in share.secrets.iter() {
            //                             if roots.contains_key(&ss.dealer){
            //                                 let rs = roots.get(&ss.dealer).unwrap();
            //                                 if Shares::verify_merkle(ss.y,ss.merkle_proof.clone(),rs[share.index - 1],ss.total_party_num) {
            //                                     let mut write_lock = shares_verified.write().await;
            //                                     let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
            //                                     let temp2 = temp.entry(ss.c).or_insert(HashSet::new());
            //                                     temp2.insert((ss.dealer,ss.y));
            //                                     info!("share has been verified,directly insert in the shares_verified");
            //                                     shares_verified_watch_sender.send(()).unwrap();
            //                                 }else {
            //                                     info!("share is fake");
            //                                 }
            //                             }else {
            //                                 info!("root has not been received yet, insert into shares_unverified_yet");
            //                                 let mut write_lock = shares_unverified_yet.write().await;
            //                                 let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
            //                                 let temp2 = temp.entry(ss.c).or_insert(HashSet::new());
            //                                 temp2.insert((ss.dealer,ss.clone()));
            //                             }
            //                         }
            //
            //                     }
            //                     None => {
            //                         info!("x:root has not been received yet, insert into shares_unverified_yet");
            //                         for ss in share.secrets.iter() {
            //                             let mut write_lock = shares_unverified_yet.write().await;
            //                                 let temp = write_lock.entry((share.epoch,share.index)).or_insert(HashMap::new());
            //                                 let temp2 = temp.entry(ss.c).or_insert(HashSet::new());
            //                                 temp2.insert((ss.dealer,ss.clone()));
            //                         }
            //                     }
            //                 }
            //             }
            //             _ => {}
            //         }
            //     }
            // }
        }
    }
}
