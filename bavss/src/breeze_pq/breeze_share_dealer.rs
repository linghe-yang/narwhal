use std::time::Instant;
use crate::breeze_pq::calculation::*;
use crate::breeze_pq::polynomial::Polynomial;
use crate::breeze_pq::zq_int::ZqInt;
use crate::breeze_structs::{PQCrs, ProofUnit, Share};
use crate::merkletree::{generate_merkle_tree, verify_merkle_proof};
use crypto::{Digest, PublicKey};
use log::error;
use model::types_and_const::{Epoch, Id, ZqMod};
use nalgebra::DVector;
use rayon::iter::ParallelIterator;
use rayon::prelude::ParallelBridge;
use sha2::{Digest as ShaDigest, Sha256};

pub struct Shares(pub(crate) Vec<(Share, PublicKey)>);
impl Shares {
    pub fn get_c_ref(&self) -> &Digest {
        &self.0[0].0.c
    }
    pub fn get_shares_ref(&self) -> &Vec<(Share, PublicKey)> {
        &self.0
    }
    pub fn verify_shares(
        crs: &PQCrs,
        share: &Share,
        id: Id,
    ) -> bool {
        let proofs: Vec<_> = share.eval_proof.iter().map(|p| ProofUnit::from_residue_vecs(p, crs.q)).collect();
        let t = t_vec_to_dvec(&share.t, crs.q);
        let u = u_vec_to_dvec(&share.y_k, crs.q, crs.kappa * crs.n);
        let x = generate_x_vectors(ZqInt::new(id as ZqMod, crs.q), crs.ell, crs.r);
        #[cfg(feature = "eval")]
        let start = Instant::now();
        let res = verify_proofs(&proofs, &crs.a, t, u, x, crs.ell, crs.r, crs.kappa, crs.n, crs.q, crs.log_q);
        #[cfg(feature = "eval")]
        let _duration = start.elapsed();
        #[cfg(feature = "eval")]
        {
            println!("Verification time: {:?}", _duration);
            let batch_size = crs.kappa * crs.n / crs.g;
            println!("Verification time per beacon: {:?}", _duration / batch_size as u32);
        }

        res
    }


    pub fn verify_merkle_batch(id: usize, share: &Share, roots: &Vec<Digest>) -> bool {
        let mut flag = true;
        for (idx, x) in share.y_k.iter().enumerate() {
            let mut hasher = Sha256::new();
            hasher.update(x.to_be_bytes());
            let leaf = hasher.finalize().to_vec();
            match verify_merkle_proof(
                &leaf,
                (id - 1, share.merkle_proofs[idx].clone()),
                roots[idx],
                share.total_party_num,
            ) {
                Ok(_) => {}
                Err(_) => {
                    flag = false;
                    break;
                }
            }
        }
        flag
    }
    pub fn verify_merkle(y: &Vec<ZqMod>, proof: (usize,Vec<Vec<u8>>), root: Vec<Digest>, total_leaves_count: usize) -> bool {
        if proof.1.len() != root.len() || y.len() != root.len() {
            error!("proof length and roots length mismatch");
            return false;
        }

        for (i,p) in proof.1.iter().enumerate() {
            let mut hasher = Sha256::new();
            hasher.update(y[i].to_be_bytes());
            let leaf = hasher.finalize().to_vec();
            match verify_merkle_proof(
                &leaf,
                (proof.0,p.clone()),
                root[i],
                total_leaves_count
            ) {
                Ok(_) => {}
                Err(_) => {
                    return false;
                }
            }
        }
        true

    }
    pub fn new(
        batch_size: usize,
        epoch: Epoch,
        ids: Vec<(PublicKey, Id)>,
        ft: usize,
        crs: &PQCrs,
    ) -> (Self, Vec<Digest>) {
        let g = crs.g;
        let q = crs.q;
        let log_q = crs.log_q;
        let r: usize = crs.r;
        let ell = crs.ell;
        let kappa = crs.kappa;
        let n = crs.n;
        let mut polynomials = Vec::new();
        assert!(batch_size * g <= kappa * n, "batch size too large");
        for _ in 0..(batch_size * g) {
            polynomials.push(Polynomial::new(ft, q));
        }
        let f = generate_f_vector(r, ell, kappa, n, q, polynomials);
        let a = &crs.a;
        let mut s_vectors: Vec<Option<DVector<ZqInt>>> = vec![None; ell + 1];
        #[cfg(feature = "eval")]
        let start = Instant::now();
        let t = generate_t(&f, &mut s_vectors, r, ell, kappa, n, q, log_q, &a);
        #[cfg(feature = "eval")]
        let _duration = start.elapsed();
        #[cfg(feature = "eval")]
        {
            println!("Commitment generation time: {:?}", _duration);
            println!("Commitment generation time per beacon: {:?}", _duration / batch_size as u32);
        }


        let s_vectors: Vec<DVector<ZqInt>> = s_vectors
            .into_iter()
            .map(|opt| opt.unwrap())
            .collect();

        let t_vec = t_dvec_2_t_vec(&t);
        let t_vec_hash = hash_c(&t_vec);
        #[cfg(feature = "eval")]
        let start = Instant::now();
        let chunk_size = ids.len() / 10 + 1;
        let mut shares: Vec<_> = ids
            .chunks(chunk_size)
            .par_bridge()
            .flat_map(|chunk| {
                chunk
                    .iter()
                    .map(|&(pk, id)| {
                        let x = generate_x_vectors(ZqInt::new(id as ZqMod, q), ell, r);
                        let u =
                            generate_polynomial_evaluation(f.clone(), &x, r, ell, ell, kappa, n, q);

                        let mut v0 = generate_v(f.clone(), &x, r, ell, ell, 1, kappa, n, q);
                        let c = generate_fiat_shamir_challenge_matrix(
                            &t,
                            &u,
                            &x,
                            &s_vectors[0],
                            &v0,
                            r,
                            kappa,
                            q,
                        );
                        let mut proof: Vec<ProofUnit> = Vec::new();
                        proof.push(ProofUnit::new(s_vectors[0].clone(), v0.clone()));
                        generate_proof(
                            &mut proof,
                            &mut x.clone(),
                            &mut f.clone(),
                            &mut s_vectors.clone(),
                            &mut s_vectors[0].clone(),
                            &mut v0,
                            c,
                            ell,
                            1,
                            r,
                            n,
                            kappa,
                            q,
                            log_q,
                        );
                        let share = Share {
                            t: t_vec.clone(),
                            c: t_vec_hash.clone(),
                            y_k: u_dvec_2_u_vec(&u, batch_size * g),
                            merkle_proofs: Vec::default(),
                            eval_proof: proof_unit_to_vec(&proof),
                            epoch,
                            total_party_num: ids.len(),
                        };
                        (share, pk)
                    })
                    .collect::<Vec<_>>() // 在每个块内串行处理
            })
            .collect();
        #[cfg(feature = "eval")]
        let _duration = start.elapsed();
        #[cfg(feature = "eval")]
        {
            println!("Proof generation time: {:?}", _duration);
            let duration_per_beacon = _duration / batch_size as u32;
            println!("Proof generation time per beacon: {:?}", duration_per_beacon);
        }

        let (roots, proofs) = generate_merkle_proofs(&shares);
        for (share, proof) in shares.iter_mut().zip(proofs.into_iter()) {
            share.0.merkle_proofs = proof.1;
        }
        (Shares(shares), roots)
    }
}

fn t_dvec_2_t_vec(v: &DVector<ZqInt>) -> Vec<ZqMod> {
    let res: Vec<_> = v.iter().map(|&z| z.residue()).collect();
    res
}
fn u_dvec_2_u_vec(v: &DVector<ZqInt>, g_batch_size: usize) -> Vec<ZqMod> {
    let res: Vec<_> = v.iter().take(g_batch_size).map(|&z| z.residue()).collect();
    res
}
fn proof_unit_to_vec(v: &Vec<ProofUnit>) -> Vec<(Vec<ZqMod>, Vec<ZqMod>)> {
    let res: Vec<_> = v.iter().map(|p| p.to_residue_vecs()).collect();
    res
}
fn generate_merkle_proofs(
    shares: &Vec<(Share, PublicKey)>,
) -> (Vec<Digest>, Vec<(usize, Vec<Vec<u8>>)>) {
    let share_vec: Vec<_> = shares.iter().map(|s| &s.0).collect();
    let mut roots = Vec::new();
    let mut proofs = Vec::new();
    for idx in 0..share_vec[0].y_k.len() {
        let mut layer = Vec::new();
        for share in share_vec.iter() {
            layer.push(share.y_k[idx]);
        }
        let leaves = hash_u128_vec_to_bytes_vec(layer);
        let (root, merkle_proof) = match generate_merkle_tree(leaves) {
            Ok(res) => res,
            Err(_) => {
                error!("failed to generate merkle tree of layer {}", idx);
                continue;
            }
        };
        roots.push(root);
        proofs.push(merkle_proof);
    }
    (roots, transpose_merkle_proofs(proofs))
}

fn hash_u128_vec_to_bytes_vec(inputs: Vec<ZqMod>) -> Vec<Vec<u8>> {
    inputs
        .into_iter()
        .map(|input| {
            let mut hasher = Sha256::new();
            hasher.update(&input.to_be_bytes());
            hasher.finalize().to_vec()
        })
        .collect()
}

fn hash_c(t: &Vec<ZqMod>) -> Digest {
    let mut hasher = Sha256::new();
    for num in t {
        hasher.update(num.to_be_bytes());
    }
    let result = hasher.finalize();
    let mut output = [0u8; 32];
    output.copy_from_slice(&result);
    Digest(output)
}

fn transpose_merkle_proofs(matrix: Vec<Vec<(usize, Vec<u8>)>>) -> Vec<(usize, Vec<Vec<u8>>)> {
    if matrix.is_empty() || matrix[0].is_empty() {
        return Vec::new();
    }
    
    let b = matrix.len();
    let n = matrix[0].len();
    
    for row in &matrix {
        if row.len() != n {
            panic!("Invalid matrix: rows have different lengths");
        }
        for (j, &(idx, _)) in row.iter().enumerate() {
            if idx != j {
                panic!("Invalid matrix: usize values must be 0 to n-1");
            }
        }
    }
    
    let mut result = Vec::with_capacity(n);
    for j in 0..n {
        let mut column = Vec::with_capacity(b);
        for i in 0..b {
            column.push(matrix[i][j].1.clone());
        }
        result.push((j + 1, column));
    }
    result
}
fn u_vec_to_dvec(vec: &Vec<ZqMod>, q: ZqMod, kappa_n: usize) -> DVector<ZqInt> {
    if vec.len() > kappa_n {
        panic!("Input vector length {} exceeds kappa_n {}", vec.len(), kappa_n);
    }
    let mut result: Vec<ZqInt> = vec.iter().map(|&ele| ZqInt::new(ele, q)).collect();
    result.resize(kappa_n, ZqInt::new(0, q));
    DVector::from_vec(result)
}
fn t_vec_to_dvec(vec: &Vec<ZqMod>, q: ZqMod) -> DVector<ZqInt> {
    DVector::from_vec(vec.iter().map(|&ele| ZqInt::new(ele, q)).collect())
}