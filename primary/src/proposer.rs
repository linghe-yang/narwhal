// Copyright(C) Facebook, Inc. and its affiliates.
use crate::messages::Metadata;
use crate::messages::{Certificate, Header};
use config::{Committee};
use crypto::Hash as _;
use crypto::{Digest, PublicKey, SignatureService};
#[cfg(feature = "benchmark")]
use log::info;
use log::{debug, log_enabled};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{watch, RwLock};
use tokio::time::{sleep, Duration, Instant};
use model::breeze_universal::BreezeCertificate;
use model::types_and_const::{round_to_epoch_index, Epoch, Round, WorkerId, MAX_EPOCH};

#[cfg(test)]
#[path = "tests/proposer_tests.rs"]
pub mod proposer_tests;

/// The proposer creates new headers and send them to the core for broadcasting and further processing.
pub struct Proposer {
    /// The public key of this primary.
    name: PublicKey,
    /// Service to sign headers.
    signature_service: SignatureService,
    /// The size of the headers' payload.
    header_size: usize,
    /// The maximum delay to wait for batches' digests.
    max_header_delay: u64,

    /// Receives the parents to include in the next header (along with their round number).
    rx_core: Receiver<(Vec<Digest>, Round)>,
    /// Receives the batches' digests from our workers.
    rx_workers: Receiver<(Digest, WorkerId)>,
    /// Sends newly created headers to the `Core`.
    tx_core: Sender<Header>,
    /// The current consensus round.
    rx_consensus: Receiver<Metadata>,

    /// The current round of the dag.
    round: Round,
    /// Holds the certificates' ids waiting to be included in the next header.
    last_parents: Vec<Digest>,
    /// Holds the batches' digests waiting to be included in the next header.
    digests: Vec<(Digest, WorkerId)>,
    /// Keeps track of the size (in bytes) of batches' digests that we received so far.
    payload_size: usize,
    /// The metadata to include in the next header.
    metadata: VecDeque<Metadata>,

    breeze_cer_buffer: Arc<RwLock<Vec<BreezeCertificate>>>,
    bcb_change_receiver: watch::Receiver<()>,
    breeze_cer_proposed: HashSet<Epoch>,
    
    flag: bool,
    breeze_epoch_limit: u64
}

impl Proposer {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        committee: &Committee,
        signature_service: SignatureService,
        header_size: usize,
        max_header_delay: u64,
        breeze_epoch_limit: u64,
        rx_core: Receiver<(Vec<Digest>, Round)>,
        rx_workers: Receiver<(Digest, WorkerId)>,
        tx_core: Sender<Header>,
        rx_consensus: Receiver<Metadata>,

        mut cer_to_consensus_receiver: Receiver<BreezeCertificate>,
    ) {
        let genesis = Certificate::genesis(committee)
            .iter()
            .map(|x| x.digest())
            .collect();

        let breeze_cer_buffer = Arc::new(RwLock::new(Vec::new()));
        let bcb = Arc::clone(&breeze_cer_buffer);

        let (bcb_change_sender, bcb_change_receiver) = watch::channel(());
        tokio::spawn(async move{
           loop {
               let cer =  cer_to_consensus_receiver.recv().await.unwrap();
               let mut write_lock = bcb.write().await;
               write_lock.push(cer);
               drop(write_lock);
               bcb_change_sender.send(()).unwrap();
           }
        });
        tokio::spawn(async move {
            Self {
                name,
                signature_service,
                header_size,
                max_header_delay,
                rx_core,
                rx_workers,
                tx_core,
                rx_consensus,
                round: 1,
                last_parents: genesis,
                digests: Vec::with_capacity(2 * header_size),
                payload_size: 0,
                metadata: VecDeque::new(),

                breeze_cer_buffer,
                bcb_change_receiver,
                breeze_cer_proposed: HashSet::new(),
                
                flag:false,
                breeze_epoch_limit
            }
            .run()
            .await;
        });
    }

    async fn make_header(&mut self) {
        let max_epoch = *MAX_EPOCH.get().unwrap();
        let (epoch ,index) = round_to_epoch_index(self.round, max_epoch);
        let mut cer: Option<BreezeCertificate> = None;

        if index == 1{
            let mut flag = false;
            for e in self.breeze_cer_proposed.iter() {
                if *e >= epoch {
                    flag = true;
                    break;
                }
            }
            if flag == false {
                info!("breeze cer for epoch:{} has not been received, start waiting", epoch);
                loop{
                    if self.bcb_change_receiver.changed().await.is_ok(){
                        let mut bcb = self.breeze_cer_buffer.write().await;
                        for (i, c) in bcb.iter().enumerate() {
                            if c.epoch == epoch {
                                cer = Some(c.clone());
                                self.breeze_cer_proposed.insert(c.epoch);
                                bcb.remove(i);
                                break;
                            }
                        }
                        if cer != None {
                            info!("breeze cer for epoch:{} has been received, recover dag", epoch);
                            break;
                        }
                    }
                }
            }
        }
        else {
            if self.breeze_epoch_limit < 1{
                let mut bcb = self.breeze_cer_buffer.write().await;
                cer = if bcb.is_empty() {
                    None
                } else {
                    let bc = bcb.remove(0);
                    self.breeze_cer_proposed.insert(bc.epoch);
                    Some(bc)
                };
                drop(bcb);
            }else {
                cer = get_certificate(&mut self.breeze_cer_proposed, &self.breeze_cer_buffer, epoch, self.breeze_epoch_limit).await;
            }
        }
        // Make a new header.
        let header = Header::new(
            self.name,
            self.round,
            self.digests.drain(..).collect(),
            self.last_parents.drain(..).collect(),
            self.metadata.pop_back(),
            &mut self.signature_service,

            cer,
        )
        .await;
        debug!("Created {:?}", header);
        if log_enabled!(log::Level::Debug) {
            if let Some(metadata) = header.metadata.as_ref() {
                debug!(
                    "{} contains virtual round {}",
                    header, metadata.virtual_round
                );
                debug!(
                    "{} virtual parents are {:?}",
                    header, metadata.virtual_parents
                );
            }
        }

        #[cfg(feature = "benchmark")]
        for digest in header.payload.keys() {
            // NOTE: This log entry is used to compute performance.
            info!("Created {} -> {:?}", header, digest);
        }

        // Send the new header to the `Core` that will broadcast and process it.
        self.tx_core
            .send(header)
            .await
            .expect("Failed to send header");
    }

    // Main loop listening to incoming messages.
    pub async fn run(&mut self) {
        debug!("Dag starting at round {}", self.round);

        let timer = sleep(Duration::from_millis(self.max_header_delay));
        tokio::pin!(timer);

        loop {
            // Check if we can propose a new header. We propose a new header when one of the following
            // conditions is met:
            // 1. We have a quorum of certificates from the previous round and enough batches' digests;
            // 2. We have a quorum of certificates from the previous round and the specified maximum
            // inter-header delay has passed.
            let enough_parents = !self.last_parents.is_empty();
            let enough_digests = self.payload_size >= self.header_size;
            let timer_expired = timer.is_elapsed();
            // let metadata_ready = !self.metadata.is_empty();
            #[cfg(feature = "dolphin")]
            let metadata_ready = !self.metadata.is_empty();
            #[cfg(not(feature = "dolphin"))]
            let metadata_ready = true;
            if (timer_expired || enough_digests) && enough_parents && metadata_ready && self.flag {

                // Make a new header.
                self.make_header().await;
                self.payload_size = 0;

                // Reschedule the timer.
                let deadline = Instant::now() + Duration::from_millis(self.max_header_delay);
                timer.as_mut().reset(deadline);
            }

            tokio::select! {
                Some((parents, round)) = self.rx_core.recv() => {
                    if round < self.round {
                        debug!("Dag received {} round from rx_core too small, skip", round);
                        continue;
                    }

                    // Advance to the next round.
                    self.round = round + 1;
                    debug!("Dag moved to round {}", self.round);

                    // Signal that we have enough parent certificates to propose a new header.
                    self.last_parents = parents;
                }
                Some((digest, worker_id)) = self.rx_workers.recv() => {
                    self.payload_size += digest.size();
                    self.digests.push((digest, worker_id));
                    self.flag = true;
                }
                Some(metadata) = self.rx_consensus.recv() => {
                    self.metadata.push_front(metadata);
                }
                // certificate from breeze.
                // Some(cer) = self.cer_to_consensus_receiver.recv() => {
                //     self.breeze_cer_buffer.push_back(cer);
                // }
                () = &mut timer => {
                    // Nothing to do.
                }
            }
        }
    }
}


async fn get_certificate(
    breeze_cer_proposed: &mut HashSet<Epoch>,
    breeze_cer_buffer: &Arc<RwLock<Vec<BreezeCertificate>>>,
    epoch: Epoch,
    breeze_epoch_limit: u64
) -> Option<BreezeCertificate> {
    let mut bcb = breeze_cer_buffer.write().await;

    if bcb.is_empty() {
        drop(bcb);
        return None;
    }

    // Find maximum epoch in breeze_cer_proposed
    // let max_epoch = breeze_cer_proposed.iter().max().copied().unwrap_or(0);

    // Find index of first certificate with epoch in range (max_epoch, max_epoch + 16)
    let index_to_remove = bcb.iter()
        .enumerate()
        .find(|(_, cert)| cert.epoch > epoch && cert.epoch < epoch + breeze_epoch_limit)
        .map(|(index, _)| index);

    let cer = if let Some(index) = index_to_remove {
        let bc = bcb.remove(index);
        breeze_cer_proposed.insert(bc.epoch);
        Some(bc)
    } else {
        None
    };

    drop(bcb);
    cer
}