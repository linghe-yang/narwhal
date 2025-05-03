use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
// Copyright(C) Facebook, Inc. and its affiliates.
use crate::state::{Dag, State};
use config::{Committee};
use crypto::Digest;
use log::{debug, info, log_enabled, warn};
use primary::{Certificate};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use drb_coordinator::error::DrbError;
use model::types_and_const::{RandomNum, Round, Stake};

#[cfg(test)]
#[path = "tests/tusk_tests.rs"]
pub mod tusk_tests;

pub struct Tusk {
    /// The committee information.
    committee: Committee,
    /// The depth of the garbage collector.
    gc_depth: Round,

    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_primary: Receiver<Certificate>,
    /// Outputs the sequence of ordered certificates to the primary (for cleanup and feedback).
    tx_primary: Sender<Certificate>,
    /// Outputs the sequence of ordered certificates to the application layer.
    tx_output: Sender<Certificate>,

    /// The genesis certificates.
    genesis: Vec<Certificate>,

    global_coin_recon_req_sender: Sender<Round>,
    global_coin_buffer: Arc<RwLock<HashMap<Round, RandomNum>>>,
}

impl Tusk {
    pub fn spawn(
        committee: Committee,
        gc_depth: Round,
        rx_primary: Receiver<Certificate>,
        tx_primary: Sender<Certificate>,
        tx_output: Sender<Certificate>,

        global_coin_recon_req_sender: Sender<Round>,
        mut global_coin_res_receiver: Receiver<(Round, Result<RandomNum, DrbError>)>
    ) {
        let global_coin_buffer= Arc::new(RwLock::new(HashMap::new()));
        let buffer_ref = Arc::clone(&global_coin_buffer);
        let s = global_coin_recon_req_sender.clone();

        tokio::spawn(async move {
            loop{
                let res = global_coin_res_receiver.recv().await.unwrap();
                if let Ok(random_num) = res.1 {
                    let mut write_lock = buffer_ref.write().await;
                    write_lock.insert(res.0, random_num);
                    drop(write_lock);
                } else {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    s.send(res.0).await.unwrap();
                }
            }
        });

        tokio::spawn(async move {
            Self {
                committee: committee.clone(),
                gc_depth,
                rx_primary,
                tx_primary,
                tx_output,
                genesis: Certificate::genesis(&committee),

                global_coin_recon_req_sender,
                global_coin_buffer:Arc::clone(&global_coin_buffer)
            }
            .run()
            .await;
        });
    }

    async fn run(&mut self) {
        info!("Starting Consensus...");
        // The consensus state (everything else is immutable).
        let mut state = State::new(self.gc_depth, self.genesis.clone());

        // Listen to incoming certificates.
        while let Some(certificate) = self.rx_primary.recv().await {
            debug!("Processing {:?}", certificate);
            let round = certificate.round();

            // Add the new certificate to the local storage.
            state.add(certificate);

            // Try to order the dag to commit. Start from the highest round for which we have at least
            // 2f+1 certificates. This is because we need them to reveal the common coin.
            let r = round - 1;

            // We only elect leaders for even round numbers.
            if r % 2 != 0 || r < 4 {
                continue;
            }

            // Get the certificate's digest of the leader of round r-2. If we already ordered this leader,
            // there is nothing to do.
            let leader_round = r - 2;
            if leader_round <= state.last_committed_round {
                continue;
            }
            let (leader_digest, leader) = match self.leader(leader_round, &state.dag).await {
                Some(x) => x,
                None => continue,
            };

            // Check if the leader has f+1 support from its children (ie. round r-1).
            let stake: Stake = state
                .dag
                .get(&(r - 1))
                .expect("We should have the whole history by now")
                .values()
                .filter(|(_, x)| x.header.parents.contains(&leader_digest))
                .map(|(_, x)| self.committee.stake(&x.origin()))
                .sum();

            // If it is the case, we can commit the leader. But first, we need to recursively go back to
            // the last committed leader, and commit all preceding leaders in the right order. Committing
            // a leader block means committing all its dependencies.
            if stake < self.committee.validity_threshold() {
                debug!("Leader {:?} does not have enough support", leader);
                continue;
            }

            // Get an ordered list of past leaders that are linked to the current leader.
            debug!("Leader {:?} has enough support", leader);
            let mut sequence = Vec::new();
            for leader in self.order_leaders(leader, &state).await.iter().rev() {
                // Starting from the oldest leader, flatten the sub-dag referenced by the leader.
                for x in state.flatten(leader) {
                    // Update and clean up internal state.
                    state.update(&x);

                    // Add the certificate to the sequence.
                    sequence.push(x);
                }
            }

            // Log the latest committed round of every authority (for debug).
            if log_enabled!(log::Level::Debug) {
                for (name, round) in &state.last_committed {
                    debug!("Latest commit of {}: Round {}", name, round);
                }
            }

            // Output the sequence in the right order.
            for certificate in sequence {
                #[cfg(not(feature = "benchmark"))]
                info!("Committed {}", certificate.header);

                #[cfg(feature = "benchmark")]
                for digest in certificate.header.payload.keys() {
                    // NOTE: This log entry is used to compute performance.
                    info!("Committed {} -> {:?}", certificate.header, digest);
                }

                self.tx_primary
                    .send(certificate.clone())
                    .await
                    .expect("Failed to send certificate to primary");

                if let Err(e) = self.tx_output.send(certificate).await {
                    warn!("Failed to output certificate: {}", e);
                }
            }
        }
    }

    /// Returns the certificate (and the certificate's digest) originated by the leader of the
    /// specified round (if any).
    async fn leader<'a>(&self, round: Round, dag: &'a Dag) -> Option<&'a (Digest, Certificate)> {
        // TODO: We should elect the leader of round r-2 using the common coin revealed at round r.
        // At this stage, we are guaranteed to have 2f+1 certificates from round r (which is enough to
        // compute the coin). We currently just use round-robin.
        info!("start to elect leader for round:{}", round);
        let coin;
        self.global_coin_recon_req_sender.send(round).await.unwrap();
        loop {
            match self.global_coin_buffer.read().await.get(&round) {
                Some(r) => {
                    coin = *r;
                    break;
                }
                _ => {}
            }
        }

        // Elect the leader.
        let mut keys: Vec<_> = self.committee.authorities.keys().cloned().collect();
        keys.sort();
        let leader = keys[coin as usize % self.committee.size()];

        // Return its certificate and the certificate's digest.
        dag.get(&round).map(|x| x.get(&leader)).flatten()
    }

    /// Order the past leaders that we didn't already commit.
    async fn order_leaders(&self, leader: &Certificate, state: &State) -> Vec<Certificate> {
        let mut to_commit = vec![leader.clone()];
        let mut leader = leader;
        // for r in (state.last_committed_round + 2..leader.round()) //original: r-sequence is odd numbers
        for r in (state.last_committed_round + 2..=leader.round())
            .rev()
            .step_by(2)
        {
            if r % 2 != 0 {
                panic!("r: {} error", r);
            }
            // Get the certificate proposed by the previous leader.
            let (_, prev_leader) = match self.leader(r, &state.dag).await {
                Some(x) => x,
                None => continue,
            };

            // Check whether there is a path between the last two leaders.
            if self.linked(leader, prev_leader, &state.dag) {
                to_commit.push(prev_leader.clone());
                leader = prev_leader;
            }
        }
        to_commit
    }

    /// Checks if there is a path between two leaders.
    fn linked(&self, leader: &Certificate, prev_leader: &Certificate, dag: &Dag) -> bool {
        let mut parents = vec![leader];
        for r in (prev_leader.round()..leader.round()).rev() {
            parents = dag
                .get(&(r))
                .expect("We should have the whole history by now")
                .values()
                .filter(|(digest, _)| parents.iter().any(|x| x.header.parents.contains(digest)))
                .map(|(_, certificate)| certificate)
                .collect();
        }
        parents.contains(&prev_leader)
    }
}
