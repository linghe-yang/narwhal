// Copyright(C) Facebook, Inc. and its affiliates.
use crypto::{generate_production_keypair, PublicKey, SecretKey};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use model::file_io::*;
use model::types_and_const::{Id, Stake, WorkerId};

// #[derive(Error, Debug)]
// pub enum ConfigError {
//     #[error("Node {0} is not in the committee")]
//     NotInCommittee(PublicKey),
//
//     #[error("Unknown worker id {0}")]
//     UnknownWorker(WorkerId),
//
//     #[error("Failed to read config file '{file}': {message}")]
//     ImportError { file: String, message: String },
//
//     #[error("Failed to write config file '{file}': {message}")]
//     ExportError { file: String, message: String },
// }
//
// pub trait Import: DeserializeOwned {
//     fn import(path: &str) -> Result<Self, ConfigError> {
//         let reader = || -> Result<Self, std::io::Error> {
//             let data = fs::read(path)?;
//             Ok(serde_json::from_slice(data.as_slice())?)
//         };
//         reader().map_err(|e| ConfigError::ImportError {
//             file: path.to_string(),
//             message: e.to_string(),
//         })
//     }
// }
//
// pub trait Export: Serialize {
//     fn export(&self, path: &str) -> Result<(), ConfigError> {
//         let writer = || -> Result<(), std::io::Error> {
//             let file = OpenOptions::new().create(true).write(true).open(path)?;
//             let mut writer = BufWriter::new(file);
//             let data = serde_json::to_string_pretty(self).unwrap();
//             writer.write_all(data.as_ref())?;
//             writer.write_all(b"\n")?;
//             Ok(())
//         };
//         writer().map_err(|e| ConfigError::ExportError {
//             file: path.to_string(),
//             message: e.to_string(),
//         })
//     }
// }



#[derive(Deserialize, Clone)]
pub struct Parameters {
    /// The preferred header size. The primary creates a new header when it has enough parents and
    /// enough batches' digests to reach `header_size`. Denominated in bytes.
    pub header_size: usize,
    /// The maximum delay that the primary waits between generating two headers, even if the header
    /// did not reach `max_header_size`. Denominated in ms.
    pub max_header_delay: u64,
    /// The depth of the garbage collection (Denominated in number of rounds).
    pub gc_depth: u64,
    /// The delay after which the synchronizer retries to send sync requests. Denominated in ms.
    pub sync_retry_delay: u64,
    /// Determine with how many nodes to sync when re-trying to send sync-request. These nodes
    /// are picked at random from the committee.
    pub sync_retry_nodes: usize,
    /// The preferred batch size. The workers seal a batch of transactions when it reaches this size.
    /// Denominated in bytes.
    pub batch_size: usize,
    /// The delay after which the workers seal a batch of transactions, even if `max_batch_size`
    /// is not reached. Denominated in ms.
    pub max_batch_delay: u64,
    #[cfg(feature = "dolphin")]
    /// The leader timeout value. Denominated in ms.
    pub timeout: u64,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            header_size: 1_000,
            max_header_delay: 100,
            gc_depth: 50,
            sync_retry_delay: 5_000,
            sync_retry_nodes: 3,
            batch_size: 500_000,
            max_batch_delay: 100,
            #[cfg(feature = "dolphin")]
            timeout: 5_000,
        }
    }
}

impl Import for Parameters {}

impl Parameters {
    pub fn log(&self) {
        info!("Header size set to {} B", self.header_size);
        info!("Max header delay set to {} ms", self.max_header_delay);
        info!("Garbage collection depth set to {} rounds", self.gc_depth);
        info!("Sync retry delay set to {} ms", self.sync_retry_delay);
        info!("Sync retry nodes set to {} nodes", self.sync_retry_nodes);
        info!("Batch size set to {} B", self.batch_size);
        info!("Max batch delay set to {} ms", self.max_batch_delay);
        #[cfg(feature = "dolphin")]
        info!("Leader timeout set to {} ms", self.timeout);
    }
}

#[derive(Clone, Deserialize)]
pub struct PrimaryAddresses {
    /// Address to receive messages from other primaries (WAN).
    pub primary_to_primary: SocketAddr,
    /// Address to receive messages from our workers (LAN).
    pub worker_to_primary: SocketAddr,

    pub breeze_addr: SocketAddr,
    pub init_bft_addr: SocketAddr,
}

#[derive(Clone, Deserialize, Eq, Hash, PartialEq)]
pub struct WorkerAddresses {
    /// Address to receive client transactions (WAN).
    pub transactions: SocketAddr,
    /// Address to receive messages from other workers (WAN).
    pub worker_to_worker: SocketAddr,
    /// Address to receive messages from our primary (LAN).
    pub primary_to_worker: SocketAddr,
}

#[derive(Clone, Deserialize)]
pub struct Authority {
    /// The voting power of this authority.
    pub stake: Stake,
    /// The network addresses of the primary.
    pub primary: PrimaryAddresses,
    /// Map of workers' id and their network addresses.
    pub workers: HashMap<WorkerId, WorkerAddresses>,
}

#[derive(Clone, Deserialize)]
pub struct Committee {
    pub authorities: BTreeMap<PublicKey, Authority>,
}

impl Import for Committee {}

impl Committee {
    /// Returns the number of authorities.
    pub fn size(&self) -> usize {
        self.authorities.len()
    }

    /// Return the stake of a specific authority.
    pub fn stake(&self, name: &PublicKey) -> Stake {
        self.authorities.get(&name).map_or_else(|| 0, |x| x.stake)
    }

    /// Returns the stake of all authorities except `myself`.
    pub fn others_stake(&self, myself: &PublicKey) -> Vec<(PublicKey, Stake)> {
        self.authorities
            .iter()
            .filter(|(name, _)| name != &myself)
            .map(|(name, authority)| (*name, authority.stake))
            .collect()
    }

    /// Returns the quorum (2f+1).
    pub fn authorities_quorum_threshold(&self) -> usize {
        (2 * self.authorities.len() + 1) / 3
    }

    /// Returns the fault tolerance(f)
    pub fn authorities_fault_tolerance(&self) -> usize {
        (self.authorities.len() - 1) / 3
    }

    /// Returns the stake required to reach a quorum (2f+1).
    pub fn quorum_threshold(&self) -> Stake {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (2 N + 3) / 3 = 2f + 1 + (2k + 2)/3 = 2f + 1 + k = N - f
        let total_votes: Stake = self.authorities.values().map(|x| x.stake).sum();
        2 * total_votes / 3 + 1
    }

    /// Returns the stake required to reach availability (f+1).
    pub fn validity_threshold(&self) -> Stake {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (N + 2) / 3 = f + 1 + k/3 = f + 1
        let total_votes: Stake = self.authorities.values().map(|x| x.stake).sum();
        (total_votes + 2) / 3
    }

    pub fn get_id(&self, key: &PublicKey) -> Option<Id> {
        self.authorities
            .keys()
            .position(|k| k == key)
            .map(|idx| idx + 1)
    }

    pub fn get_all_ids(&self) -> Vec<(PublicKey, Id)> {
        self.authorities
            .keys()
            .enumerate()
            .map(|(idx, key)| (*key, idx + 1)) // 加1因为要求从1开始
            .collect()
    }

    /// Returns the primary addresses of the target primary.
    pub fn primary(&self, to: &PublicKey) -> Result<PrimaryAddresses, ConfigError> {
        self.authorities
            .get(to)
            .map(|x| x.primary.clone())
            .ok_or_else(|| ConfigError::NotInCommittee(*to))
    }

    /// Returns the breeze address of the target primary.
    pub fn breeze_address(&self, to: &PublicKey) -> Result<SocketAddr, ConfigError> {
        self.authorities
            .get(to)
            .map(|x| x.primary.breeze_addr.clone())
            .ok_or_else(|| ConfigError::NotInCommittee(*to))
    }

    pub fn init_bft_address(&self, to: &PublicKey) -> Result<SocketAddr, ConfigError> {
        self.authorities
            .get(to)
            .map(|x| x.primary.init_bft_addr.clone())
            .ok_or_else(|| ConfigError::NotInCommittee(*to))
    }

    /// Returns the addresses of all primaries except `myself`.
    pub fn others_primaries(&self, myself: &PublicKey) -> Vec<(PublicKey, PrimaryAddresses)> {
        self.authorities
            .iter()
            .filter(|(name, _)| name != &myself)
            .map(|(name, authority)| (*name, authority.primary.clone()))
            .collect()
    }
    /// Returns all the breeze addresses
    pub fn all_breeze_addresses(&self) -> Vec<(PublicKey, SocketAddr)> {
        self.authorities
            .iter()
            .map(|(name, authority)| (*name, authority.primary.breeze_addr.clone()))
            .collect()
    }

    pub fn all_init_bft_addresses(&self) -> Vec<(PublicKey, SocketAddr)> {
        self.authorities
            .iter()
            .map(|(name, authority)| (*name, authority.primary.init_bft_addr.clone()))
            .collect()
    }
    /// Returns the addresses of a specific worker (`id`) of a specific authority (`to`).
    pub fn worker(&self, to: &PublicKey, id: &WorkerId) -> Result<WorkerAddresses, ConfigError> {
        self.authorities
            .iter()
            .find(|(name, _)| name == &to)
            .map(|(_, authority)| authority)
            .ok_or_else(|| ConfigError::NotInCommittee(*to))?
            .workers
            .iter()
            .find(|(worker_id, _)| worker_id == &id)
            .map(|(_, worker)| worker.clone())
            .ok_or_else(|| ConfigError::NotInCommittee(*to))
    }

    /// Returns the addresses of all our workers.
    pub fn our_workers(&self, myself: &PublicKey) -> Result<Vec<WorkerAddresses>, ConfigError> {
        self.authorities
            .iter()
            .find(|(name, _)| name == &myself)
            .map(|(_, authority)| authority)
            .ok_or_else(|| ConfigError::NotInCommittee(*myself))?
            .workers
            .values()
            .cloned()
            .map(Ok)
            .collect()
    }

    /// Returns the addresses of all workers with a specific id except the ones of the authority
    /// specified by `myself`.
    pub fn others_workers(
        &self,
        myself: &PublicKey,
        id: &WorkerId,
    ) -> Vec<(PublicKey, WorkerAddresses)> {
        self.authorities
            .iter()
            .filter(|(name, _)| name != &myself)
            .filter_map(|(name, authority)| {
                authority
                    .workers
                    .iter()
                    .find(|(worker_id, _)| worker_id == &id)
                    .map(|(_, addresses)| (*name, addresses.clone()))
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct KeyPair {
    /// The node's public key (and identifier).
    pub name: PublicKey,
    /// The node's secret key.
    pub secret: SecretKey,
}

impl Import for KeyPair {}
impl Export for KeyPair {}

impl KeyPair {
    pub fn new() -> Self {
        let (name, secret) = generate_production_keypair();
        Self { name, secret }
    }
}

impl Default for KeyPair {
    fn default() -> Self {
        Self::new()
    }
}
