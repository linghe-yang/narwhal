// Copyright(C) Facebook, Inc. and its affiliates.
use crate::state::Dag;
use config::Committee;
use crypto::{Digest, Hash as _, PublicKey};
use log::{debug};
use primary::{Certificate};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use drb_coordinator::error::DrbError;
use model::types_and_const::{RandomNum, Round};

/// The virtual consensus state. This state is interpreted from metadata included in the certificates
/// and can be derived from the real state (`State`).
pub struct VirtualState {
    /// The committee information.
    committee: Committee,
    /// Keeps the latest committed certificate (and its children) for every authority. Anything older
    /// must be regularly cleaned up through the function `update`.
    pub dag: Dag,
    /// Keeps tracks of authorities is in the steady state.
    pub steady_authorities_sets: HashMap<Round, HashSet<PublicKey>>,
    /// Keeps tracks of authorities in the fallback state.
    pub fallback_authorities_sets: HashMap<Round, HashSet<PublicKey>>,

    pub _steady_state: bool,

    pub global_coin_recon_req_sender: Sender<Round>,
    pub global_coin_buffer: Arc<RwLock<HashMap<Round, RandomNum>>>,
}

impl VirtualState {
    /// Create a new (empty) virtual state.
    pub fn new(committee: Committee, genesis: Vec<Certificate>, global_coin_recon_req_sender: Sender<Round>, mut global_coin_res_receiver: Receiver<(Round,Result<RandomNum, DrbError>)>) -> Self {
        let genesis = genesis
            .into_iter()
            .map(|x| (x.origin(), (x.digest(), x)))
            .collect::<HashMap<_, _>>();

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
                }else {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    s.send(res.0).await.unwrap();
                }
            }
        });

        Self {
            committee: committee.clone(),
            dag: [(0, genesis)].iter().cloned().collect(),
            steady_authorities_sets: [(1, committee.authorities.keys().cloned().collect())]
                .iter()
                .cloned()
                .collect(),
            fallback_authorities_sets: HashMap::new(),
            _steady_state: true,

            global_coin_recon_req_sender,
            global_coin_buffer:Arc::clone(&global_coin_buffer)
        }
    }

    /// Try to a certificate to the virtual dag and return its success status.
    pub fn try_add(&mut self, certificate: &Certificate) -> bool {
        let round = certificate.virtual_round();

        // Ensure the certificate contains virtual metadata.
        if certificate.header.metadata.is_none() {
            return false;
        }

        // Ensure the virtual metadata are correct. Particularly, ensure all parents are from the previous
        // round and that one of the parents is from the same author as the certificate.
        let previous_round_certificates: Vec<_> = self
            .dag
            .get(&(round - 1))
            .map_or_else(Vec::default, |x| x.values().map(|(x, _)| x).collect());

        let ok = certificate
            .virtual_parents()
            .iter()
            .all(|x| previous_round_certificates.contains(x));
        //&& self
        //    .dag
        //    .get(&(round - 1))
        //    .map_or_else(|| false, |x| x.contains_key(&certificate.origin()));

        // Add the certificate to the dag.
        if ok {
            self.dag.entry(round).or_insert_with(HashMap::new).insert(
                certificate.origin(),
                (certificate.digest(), certificate.clone()),
            );
        }

        ok
    }

    /// Cleanup the internal state after committing a certificate.
    pub fn cleanup(&mut self, last_committed_round: Round, gc_depth: Round) {
        self.dag.retain(|r, _| r + gc_depth > last_committed_round);
        self.steady_authorities_sets
            .retain(|w, _| w >= &((last_committed_round + 1) / 2));
        self.fallback_authorities_sets
            .retain(|w, _| w >= &((last_committed_round + 1) / 4));
    }

    /// Returns the certificate (and the certificate's digest) originated by the steady-state leader
    /// of the specified round (if any).
    pub fn steady_leader(&self, wave: Round) -> Option<&(Digest, Certificate)> {
        #[cfg(test)]
        let seed = 0;
        #[cfg(not(test))]
        let seed = wave;

        // Elect the leader.
        let mut keys: Vec<_> = self.committee.authorities.keys().cloned().collect();
        keys.sort();
        let leader = keys[seed as usize % self.committee.size()];

        // Return its certificate and the certificate's digest.
        let round = match wave {
            0 => 0,
            _ => wave * 2 - 1,
        };
        self.dag.get(&round).map(|x| x.get(&leader)).flatten()
    }

    /// Returns the certificate (and the certificate's digest) originated by the fallback leader
    /// of the specified round (if any).
    pub async fn fallback_leader(&self, wave: Round) -> Option<&(Digest, Certificate)> {
        // Done: TODO: We should elect the leader of round r-2 using the common coin revealed at round r.
        // At this stage, we are guaranteed to have 2f+1 certificates from round r (which is enough to
        // compute the coin). We currently just use round-robin.

        // We use randomness beacon to get global coin.
        let wave = if wave % 2 == 0 {wave -1 } else { wave };

        let coin;
        self.global_coin_recon_req_sender.send(wave).await.unwrap();
        loop {
            match self.global_coin_buffer.read().await.get(&wave) {
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
        let round = match wave {
            0 => 0,
            _ => wave * 2 - 1,
        };
        self.dag.get(&round).map(|x| x.get(&leader)).flatten()
    }

    /// Print the mode and latest waves of each authority.
    pub fn print_status(&self, certificate: &Certificate) {
        let mut seen = HashSet::new();
        let steady_wave = (certificate.virtual_round() + 1) / 2;
        for w in (1..=steady_wave).rev() {
            if let Some(nodes) = self.steady_authorities_sets.get(&w) {
                for node in nodes {
                    if seen.insert(node) {
                        debug!("Latest steady wave of {}: {}", node, w);
                    }
                }
            }
            if seen.len() == self.committee.size() {
                break;
            }
        }

        seen.clear();
        let fallback_wave = (certificate.virtual_round() + 1) / 4;
        for w in (1..=fallback_wave).rev() {
            if let Some(nodes) = self.fallback_authorities_sets.get(&w) {
                for node in nodes {
                    if seen.insert(&node) {
                        debug!("Latest fallback wave of {}: {}", node, w);
                    }
                }
            }
            if seen.len() == self.committee.size() {
                break;
            }
        }
    }
}
