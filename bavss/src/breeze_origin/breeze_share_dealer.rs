
use curve25519_dalek::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use crypto::{Digest, PublicKey};
use model::breeze_universal::CommonReferenceString;
use model::types_and_const::{Epoch, Id};
use crate::breeze_origin::batch_eval::{batch_eval, batch_verify_eval};
use crate::breeze_origin::merkletree::{generate_merkle_tree, verify_merkle_proof};
use crate::breeze_origin::utils::transpose;
use crate::breeze_structs::{Share, WitnessBreeze};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shares (pub(crate) Vec<(Share, PublicKey)>);

impl Shares {
    pub fn get_c(&self) -> Digest{
        self.0[0].0.c.clone()
    }
    fn generate_batched_polynomial(batch: usize, t: usize, mut rng: OsRng) -> Vec<Vec<Scalar>> {
        let batched_polynomial: Vec<Vec<Scalar>> = (0..batch)
            .map(|_| (0..t + 1).map(|_| Scalar::random(&mut rng)).collect())
            .collect();

        batched_polynomial
    }
    fn batch_commit(
        crs: &CommonReferenceString,
        poly_r: &Vec<Vec<Scalar>>,
        t: usize,
    ) -> Vec<RistrettoPoint> {
        let poly_count = poly_r.len();

        let mut b_com = vec![RistrettoPoint::identity(); poly_count];
        assert_eq!(crs.g.len(), t + 1);
        for i in 0..poly_count {
            for j in 0..t + 1 {
                let batch_com_temp = crs.g[j] * poly_r[i][j];
                b_com[i] = b_com[i] + batch_com_temp;
            }
        }

        b_com
    }
    fn serialize_commitments(commitments: &Vec<RistrettoPoint>) -> Vec<Vec<u8>> {
        let mut data = Vec::with_capacity(commitments.len());
        for commitment in commitments {
            let bytes = commitment.compress().to_bytes();
            data.push(bytes.to_vec());
        }

        data
    }
    pub fn generate_evaluation_points_n(t: usize, ids: &Vec<(PublicKey,Id)>) -> Vec<Vec<Scalar>> {
        let mut res: Vec<Vec<Scalar>> = vec![Vec::new(); t + 1];

        for i in 0..t + 1 {
            for (_,id) in ids {
                let base = Scalar::from(*id as u64);
                let temp = Self::pow_scalar(base, i);
                res[i].push(temp);
            }
        }
        res
    }
    pub fn generate_evaluation_points_for_verifier(t: usize, id: Id) -> Vec<Scalar> {
        let mut res: Vec<Scalar> = Vec::new();

        for i in 0..t + 1 {
            let base = Scalar::from(id as u64);
            let temp = Self::pow_scalar(base, i);
            res.push(temp);
        }
        res
    }

    fn pow_scalar(base: Scalar, exp: usize) -> Scalar {
        let mut result = Scalar::ONE;
        let mut base = base;
        let mut exp = exp;

        while exp > 0 {
            if exp & 1 == 1 {
                result = result * base;
            }
            base = base * base;
            exp >>= 1;
        }

        result
    }
    pub fn verify(crs:&CommonReferenceString,node_id: Id,t:usize, share: Share) -> bool {
        let y = Self::generate_evaluation_points_for_verifier(t,node_id);
        if !batch_verify_eval(crs, &share.r_hat, share.y_k, y, share.phi_k, t, share.n){
            return false;
        }
        let mut flag = true;
        for wit in share.r_witness.iter(){
            let commit = wit.poly_commit;
            let poly_commit_data = commit.compress().to_bytes().to_vec();
            match verify_merkle_proof(&poly_commit_data, wit.merkle_branch.clone(), share.c.clone(), share.r_witness.len()) {
                Ok(res)=>{
                    if !res{
                        flag = false;
                        break;
                    }
                }
                Err(_)=>{
                    flag = false;
                    break;
                }
            }
        }
        flag
    }

    pub fn new(
        batch_size: usize,
        epoch: Epoch,
        ids: Vec<(PublicKey,Id)>,
        t: usize,
        crs: &CommonReferenceString,
    ) -> Self {
        let rng = OsRng;
        let n = ids.len();
        let batched_polynomial = Self::generate_batched_polynomial(batch_size, t, rng);
        let r_hat_breeze = Self::batch_commit(&crs, &batched_polynomial, t);
        let data = Self::serialize_commitments(&r_hat_breeze);

        let merkle_tree_data = match generate_merkle_tree(data) {
            Ok(merkle_tree_data) => merkle_tree_data,
            Err(_) => panic!("Fail to get merkle branch!"),
        };
        let (c, merkle_proofs) = merkle_tree_data;
        let r_hat_witness: Vec<WitnessBreeze> = (0..batch_size)
            .map(|i| WitnessBreeze {
                poly_commit: r_hat_breeze[i].clone(),
                merkle_branch: merkle_proofs[i].clone()
            })
            .collect();
        let y_value = Self::generate_evaluation_points_n(t, &ids);
        let (y_k, phi_k) = batch_eval(&crs, &batched_polynomial, &y_value, &r_hat_breeze, t, n);

        let y_k = transpose(y_k);

        assert_eq!(y_k.len(), n, "shards error");
        assert_eq!(phi_k.len(), n, "shards error");

        let mut all_set = Vec::new();
        for i in 0..n {
            let share = Share {
                c,
                r_hat: r_hat_breeze.clone(),
                r_witness: r_hat_witness.clone(),
                y_k: y_k[i].clone(),
                phi_k: phi_k[i].clone(),
                n,
                epoch: epoch.clone(),
            };

            all_set.push((share, ids[i].0));
        }
        Shares(all_set)
    }
}
