// Copyright(C) Facebook, Inc. and its affiliates.
use crate::error::DagResult;
use crate::header_waiter::WaiterMessage;
use crate::messages::{Certificate, Header};
use config::Committee;
use crypto::Hash as _;
use crypto::{Digest, PublicKey};
use std::collections::HashMap;
use store::Store;
use tokio::sync::mpsc::Sender;
use model::types_and_const::Round;

/// The `Synchronizer` checks if we have all batches and parents referenced by a header. If we don't, it sends
/// a command to the `Waiter` to request the missing data.
pub struct Synchronizer {
    /// The public key of this primary.
    name: PublicKey,
    /// The persistent storage.
    store: Store,
    /// Send commands to the `HeaderWaiter`.
    tx_header_waiter: Sender<WaiterMessage>,
    /// Send commands to the `CertificateWaiter`.
    tx_certificate_waiter: Sender<Certificate>,
    /// The genesis and its digests.
    genesis: Vec<(Digest, Certificate)>,
}

impl Synchronizer {
    pub fn new(
        name: PublicKey,
        committee: &Committee,
        store: Store,
        tx_header_waiter: Sender<WaiterMessage>,
        tx_certificate_waiter: Sender<Certificate>,
    ) -> Self {
        Self {
            name,
            store,
            tx_header_waiter,
            tx_certificate_waiter,
            genesis: Certificate::genesis(committee)
                .into_iter()
                .map(|x| (x.digest(), x))
                .collect(),
        }
    }

    /// Returns `true` if we have all transactions of the payload. If we don't, we return false,
    /// synchronize with other nodes (through our workers), and re-schedule processing of the
    /// header for when we will have its complete payload.
    pub async fn missing_payload(&mut self, header: &Header) -> DagResult<bool> {
        // We don't store the payload of our own workers.
        if header.author == self.name {
            return Ok(false);
        }

        let mut missing = HashMap::new();
        for (digest, worker_id) in header.payload.iter() {
            // Check whether we have the batch. If one of our worker has the batch, the primary stores the pair
            // (digest, worker_id) in its own storage. It is important to verify that we received the batch
            // from the correct worker id to prevent the following attack:
            //      1. A Bad node sends a batch X to 2f good nodes through their worker #0.
            //      2. The bad node proposes a malformed block containing the batch X and claiming it comes
            //         from worker #1.
            //      3. The 2f good nodes do not need to sync and thus don't notice that the header is malformed.
            //         The bad node together with the 2f good nodes thus certify a block containing the batch X.
            //      4. The last good node will never be able to sync as it will keep sending its sync requests
            //         to workers #1 (rather than workers #0). Also, clients will never be able to retrieve batch
            //         X as they will be querying worker #1.
            let key = [digest.as_ref(), &worker_id.to_le_bytes()].concat();
            if self.store.read(key).await?.is_none() {
                missing.insert(digest.clone(), *worker_id);
            }
        }

        if missing.is_empty() {
            return Ok(false);
        }

        self.tx_header_waiter
            .send(WaiterMessage::SyncBatches(missing, header.clone()))
            .await
            .expect("Failed to send sync batch request");
        Ok(true)
    }

    /// Returns the parents of a header if we have them all. If at least one parent is missing,
    /// we return an empty vector, synchronize with other nodes, and re-schedule processing
    /// of the header for when we will have all the parents.
    pub async fn get_parents(&mut self, header: &Header) -> DagResult<Vec<Certificate>> {
        let mut missing = Vec::new();
        let mut parents = Vec::new();
        for digest in &header.parents {
            if let Some(genesis) = self
                .genesis
                .iter()
                .find(|(x, _)| x == digest)
                .map(|(_, x)| x)
            {
                parents.push(genesis.clone());
                continue;
            }

            match self.store.read(digest.to_vec()).await? {
                Some(certificate) => parents.push(bincode::deserialize(&certificate)?),
                None => missing.push(digest.clone()),
            };
        }

        if missing.is_empty() {
            return Ok(parents);
        }

        self.tx_header_waiter
            .send(WaiterMessage::SyncParents(missing, header.clone()))
            .await
            .expect("Failed to send sync parents request");
        Ok(Vec::new())
    }

    pub async fn get_weak_links(&mut self, header: &Header, gc_round: &Round) -> DagResult<bool> {
        if let Some(metadata) = header.metadata.as_ref() {
            let mut missing = Vec::new();
            for (digest, round) in &metadata.virtual_parents {
                if gc_round >= round || self.genesis.iter().any(|(x, _)| x == digest) {
                    continue;
                }
                if self.store.read(digest.to_vec()).await?.is_none() {
                    missing.push(digest.clone());
                }
            }
            if missing.is_empty() {
                return Ok(true);
            }
            self.tx_header_waiter
                .send(WaiterMessage::SyncParents(missing, header.clone()))
                .await
                .expect("Failed to send sync parents request");
            return Ok(false);
        }
        Ok(true)
    }

    /// Check whether we have all the ancestors of the certificate. If we don't, send the certificate to
    /// the `CertificateWaiter` which will trigger re-processing once we have all the missing data.
    pub async fn deliver_certificate(&mut self, certificate: &Certificate) -> DagResult<bool> {
        for digest in &certificate.header.parents {
            if self.genesis.iter().any(|(x, _)| x == digest) {
                continue;
            }

            if self.store.read(digest.to_vec()).await?.is_none() {
                self.tx_certificate_waiter
                    .send(certificate.clone())
                    .await
                    .expect("Failed to send sync certificate request");
                return Ok(false);
            };
        }
        Ok(true)
    }
}
