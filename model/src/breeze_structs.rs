use std::collections::{BTreeSet, HashSet};
use curve25519_dalek::{RistrettoPoint, Scalar};
use serde::{Deserialize, Serialize};
use crypto::{Digest, PublicKey, Signature};
use crate::file_io::Import;
use crate::types_and_const::{Epoch};

#[derive(Debug, Clone,Serialize,Deserialize)]
pub struct CommonReferenceString {
    pub g: Vec<RistrettoPoint>,
    pub h: RistrettoPoint,
}

impl Import for CommonReferenceString {}
#[derive(Debug)]
pub struct GroupParameters{
    pub g_vec: Vec<RistrettoPoint>,
    pub h_vec: Vec<RistrettoPoint>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share{
    pub c: Digest,
    pub r_hat: Vec<RistrettoPoint>,
    pub r_witness: Vec<WitnessBreeze>,
    pub y_k:Vec<Scalar>,
    pub phi_k: PhiElement,
    pub n: usize,
    pub epoch: Epoch,
}
#[derive(Debug, Clone, Serialize, Deserialize,PartialEq)]
pub struct SingleShare{
    pub c: Digest,
    pub y: Scalar,
}
#[derive(Clone, Serialize, Deserialize,Debug, PartialEq)]
pub struct ReconstructShare {
    pub secrets: Vec<SingleShare>,
    pub epoch: Epoch,
    pub index: usize
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessBreeze {
    pub poly_commit: RistrettoPoint,
    pub merkle_branch: (usize, Vec<u8>)
}
#[derive(Clone, Serialize, Deserialize, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BreezeCertificate {
    pub c: Digest,
    pub epoch: Epoch,
    pub certificates: BTreeSet<(PublicKey,Signature)>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BreezeReconRequest {
    pub c: HashSet<Digest>,
    pub epoch: Epoch,
    pub index: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IProofUnit{
    pub a_only: Scalar,
    pub a_tilde: Scalar,
    pub l: RistrettoPoint,
    pub r: RistrettoPoint,
    pub z: Digest,
    pub b_i: (usize, Vec<u8>),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhiElement {
    pub proofs: Vec<IProofUnit>,
    pub d_hat: RistrettoPoint,
    pub v_d_i: Scalar,
}
impl ReconstructShare {
    pub fn new(secrets:Vec<SingleShare>,epoch:Epoch,index:usize) -> Self {
        ReconstructShare {
            secrets,
            epoch,
            index
        }
    }
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
pub enum BreezeContent {
    Share(Share),
    Reply(ReplyMessage),
    Confirm(ConfirmMessage),
    Reconstruct(ReconstructShare),
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BreezeMessage {
    pub sender: PublicKey,
    pub content: BreezeContent,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyMessage{
    pub epoch: Epoch,
    pub c: Digest,
    pub signature: Signature,
    pub dealer: PublicKey,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmMessage{
    pub epoch: Epoch,
    pub cer: BreezeCertificate
}

impl BreezeMessage {
    pub fn new_share_message(pk: PublicKey, share: Share) -> Self {
        BreezeMessage {
            sender: pk,
            content: BreezeContent::Share(share),
        }
    }
    pub fn new_reply_message(dealer: PublicKey, receiver:PublicKey, c: Digest, signature: Signature, epoch: Epoch) -> Self {
        BreezeMessage {
            sender: receiver,
            content: BreezeContent::Reply(ReplyMessage{
                epoch,
                c,
                signature,
                dealer,
            }),
        }
    }

    pub fn new_confirm_message(pk: PublicKey, epoch: Epoch, cer: BreezeCertificate) -> Self {
        BreezeMessage {
            sender: pk,
            content: BreezeContent::Confirm(ConfirmMessage{
                epoch,
                cer
            }),
        }
    }

    pub fn new_reconstruct_message(pk: PublicKey, share: ReconstructShare) -> Self {
        BreezeMessage {
            sender: pk,
            content: BreezeContent::Reconstruct(share),
        }
    }
    pub fn get_epoch(&self) -> Option<Epoch> {
        match &self.content {
            BreezeContent::Share(share) => {
                Option::from(share.epoch) // 返回第一个 Share 的 epoch，如果存在
            }
            BreezeContent::Reply(rm) => {
                Option::from(rm.epoch) // 返回第一个 Share 的 epoch，如果存在
            }
            BreezeContent::Confirm(rm) => {
                Option::from(rm.epoch) // 返回第一个 Share 的 epoch，如果存在
            }
            _ => None, // 如果是 Reply，返回 None
        }
    }
}
