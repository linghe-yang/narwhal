use std::collections::{BTreeSet, HashSet};
#[cfg(not(feature = "pq"))]
use curve25519_dalek::{RistrettoPoint};
use serde::{Deserialize, Serialize};
use crypto::{Digest, PublicKey, Signature};
use crate::file_io::Import;
use crate::types_and_const::Epoch;
#[cfg(feature = "pq")]
use crate::types_and_const::ZqMod;
#[cfg(feature = "pq")]
#[derive(Debug, Clone,Serialize,Deserialize)]
pub struct CommonReferenceString {
    pub a: Vec<Vec<ZqMod>>,
    pub q: ZqMod,
    pub log_q: usize,
    pub g: usize,
    pub n: usize,
    pub kappa: usize,
    pub r: usize,
    pub ell: usize
}
#[cfg(not(feature = "pq"))]
#[derive(Debug, Clone,Serialize,Deserialize)]
pub struct CommonReferenceString {
    pub g: Vec<RistrettoPoint>,
    pub h: RistrettoPoint,
}


impl Import for CommonReferenceString {}

#[derive(Clone, Serialize, Deserialize, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BreezeCertificate {
    pub c: Digest,
    pub epoch: Epoch,
    pub certificates: BTreeSet<(PublicKey,Signature)>,
}

impl BreezeCertificate {
    pub fn new(c:Digest, pk:PublicKey, epoch: Epoch, signature: Signature) -> Self {
        let mut certificates = BTreeSet::new();
        certificates.insert((pk,signature));
        BreezeCertificate {
            c,
            epoch,
            certificates,
        }
    }

    pub fn insert(&mut self, pk:PublicKey,signature: Signature) {
        self.certificates.insert((pk,signature)); // HashSet 自动去重
    }

    pub fn get_len(&self) -> usize {
        self.certificates.len()
    }
    pub fn empty(&self) -> bool {
        self.certificates.is_empty()
    }

    pub fn verify(&self, quorum_threshold: usize) -> bool {
        if self.certificates.is_empty()
            || self.certificates.len() < quorum_threshold{
            return false;
        }
        for (pk, signature) in self.certificates.iter() {
            match signature.verify(&self.c, pk) {
                Ok(()) => {},
                Err(_) => { return false; }
            }
        }
        true
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BreezeReconRequest {
    pub c: HashSet<Digest>,
    pub epoch: Epoch,
    pub index: usize,
}