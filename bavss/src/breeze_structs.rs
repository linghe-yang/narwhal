
use curve25519_dalek::{RistrettoPoint, Scalar};
use nalgebra::DMatrix;
use serde::{Deserialize, Serialize};
use crypto::{Digest, PublicKey, Signature};
use model::breeze_universal::{BreezeCertificate, CommonReferenceString};
use model::types_and_const::Epoch;

#[cfg(feature = "pq")]
use model::types_and_const::ZqMod;
#[cfg(feature = "pq")]
use crate::breeze_pq::zq_int::ZqInt;
#[cfg(feature = "pq")]
use nalgebra::DVector;

#[cfg(feature = "pq")]
pub struct PQCrs{
    pub a: DMatrix<ZqInt>,
    pub q: ZqMod,
    pub log_q: usize,
    pub g: usize,
    pub n: usize,
    pub kappa: usize,
    pub r: usize,
    pub ell: usize
}

impl PQCrs {
    pub fn from(crs: &CommonReferenceString) -> Self {
        let a = &crs.a;
        let nrows = a.len();
        assert!(nrows > 0, "Matrix must have at least one row");
        let ncols = a[0].len();
        assert!(ncols > 0, "Matrix must have at least one column");
        assert!(a.iter().all(|row| row.len() == ncols), "All rows must have the same length");
        let flat_data: Vec<ZqInt> = a.into_iter()
            .flat_map(|row| {
                row.into_iter()
                    .map(|val| ZqInt::new(*val, crs.q))
            })
            .collect();
        Self {
            a: DMatrix::from_vec(nrows, ncols, flat_data),
            q: crs.q,
            log_q: crs.log_q,
            g: crs.g,
            n: crs.n,
            kappa: crs.kappa,
            r: crs.r,
            ell: crs.ell,
        }
    }
}

#[cfg(not(feature = "pq"))]
#[derive(Debug)]
pub struct GroupParameters{
    pub g_vec: Vec<RistrettoPoint>,
    pub h_vec: Vec<RistrettoPoint>,
}
#[cfg(not(feature = "pq"))]
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
#[cfg(feature = "pq")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share{
    pub t: Vec<ZqMod>,
    pub c: Digest,
    pub y_k:Vec<ZqMod>,
    pub merkle_proofs: Vec<Vec<u8>>,
    pub eval_proof: Vec<(Vec<ZqMod>, Vec<ZqMod>)>,
    pub epoch: Epoch,
    pub total_party_num: usize,
}
#[cfg(not(feature = "pq"))]
#[derive(Debug, Clone, Serialize, Deserialize,PartialEq)]
pub struct SingleShare{
    pub c: Digest,
    pub y: Scalar,
}

#[cfg(feature = "pq")]
#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize,)]
pub struct SingleShare{
    pub dealer: PublicKey,
    pub c: Digest,
    pub y: ZqMod,
    pub merkle_proof: (usize,Vec<u8>),
    pub total_party_num: usize,
}
#[cfg(not(feature = "pq"))]
impl SingleShare{
    pub fn verify(&self) -> bool {
        //TODO: The verification method in original Breeze seems to be unsafe
        // So we do not implement the method mentioned in Rondo
        // We just return true here
        true
    }
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
#[cfg(feature = "pq")]
#[derive(Debug, Clone)]
pub struct ProofUnit {
    pub y: DVector<ZqInt>,
    pub v: DVector<ZqInt>,
}
#[cfg(feature = "pq")]
impl ProofUnit {
    pub fn new(y: DVector<ZqInt>, v: DVector<ZqInt>) -> ProofUnit {
        Self { y, v }
    }

    pub fn to_residue_vecs(&self) -> (Vec<ZqMod>, Vec<ZqMod>) {
        let y_vec: Vec<ZqMod> = self.y.iter().map(|zq| zq.residue()).collect();
        let v_vec: Vec<ZqMod> = self.v.iter().map(|zq| zq.residue()).collect();

        (y_vec, v_vec)
    }
    pub fn from_residue_vecs(residues: &(Vec<ZqMod>, Vec<ZqMod>), modulus: ZqMod) -> Self {
        let (y_residues, v_residues) = residues;
        let y: DVector<ZqInt> = DVector::from_vec(
            y_residues.into_iter().map(|r| ZqInt::new(*r, modulus)).collect()
        );
        let v: DVector<ZqInt> = DVector::from_vec(
            v_residues.into_iter().map(|r| ZqInt::new(*r, modulus)).collect()
        );
        ProofUnit { y, v }
    }
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

#[cfg(feature = "pq")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleRoots {
    pub roots: Vec<Digest>,
    pub epoch: Epoch,
}


#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum BreezeContent {
    Share(Share),
    Merkle(MerkleRoots),
    Reply(ReplyMessage),
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

    pub fn new_merkle_message(pk: PublicKey, roots: Vec<Digest>, epoch: Epoch) -> Self {
        BreezeMessage {
            sender: pk,
            content: BreezeContent::Merkle(MerkleRoots{
                roots,
                epoch
            }),
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
            BreezeContent::Merkle(rm) => {
                Option::from(rm.epoch) // 返回第一个 Share 的 epoch，如果存在
            }
            _ => None, // 如果是 Reply，返回 None
        }
    }
}
