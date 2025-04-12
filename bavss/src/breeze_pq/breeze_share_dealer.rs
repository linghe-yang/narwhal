
use rayon::iter::ParallelIterator;
use std::time::Instant;
use log::error;
use crate::breeze_pq::polynomial::Polynomial;
use crate::breeze_pq::zq_int::ZqInt;
use crypto::{Digest, PublicKey};
use model::breeze_universal::CommonReferenceString;
use model::types_and_const::{Epoch, Id, ZqMod};
use nalgebra::DVector;
use rayon::prelude::ParallelBridge;
use sha2::{Digest as ShaDigest, Sha256};
use crate::breeze_pq::calculation::{generate_a_matrix, generate_f_vector, generate_fiat_shamir_challenge_matrix, generate_polynomial_evaluation, generate_proof, generate_t, generate_v, generate_x_vectors};
use crate::breeze_structs::{ProofUnit, Share};
use crate::merkletree::generate_merkle_tree;

pub struct Shares (pub(crate) Vec<(Share, PublicKey)>);
impl Shares {
    pub fn get_c_ref(&self) -> &Digest{
        &self.0[0].0.c
    }
    pub fn get_shares_ref(&self) -> &Vec<(Share, PublicKey)> {
        &self.0
    }
    pub fn verify_shares(_crs: &CommonReferenceString, _node_id: Id, _t: usize, _share: &Share) -> bool {
        true
    }
    pub fn verify_merkles(share: &Share, roots: &Vec<Digest>) -> bool {
        true
    }
    pub fn new(
        batch_size: usize,
        epoch: Epoch,
        ids: Vec<(PublicKey, Id)>,
        f: usize,
        crs: &CommonReferenceString,
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
            polynomials.push(Polynomial::new(f, q));
        }
        let f = generate_f_vector(r, ell, kappa, n, q, polynomials);
        let a = generate_a_matrix(&crs.a, q);
        let mut s_vectors: Vec<Option<DVector<ZqInt>>> = vec![None; ell + 1];

        let start = Instant::now();
        let t = generate_t(&f, &mut s_vectors, r, ell, kappa, n, q,log_q, &a);
        println!("承诺生成时间: {:?}", start.elapsed());

        let s_vectors: Vec<DVector<ZqInt>> = s_vectors
            .into_iter()
            .map(|opt| opt.unwrap()) // 如果有 None，这里会 panic
            .collect();
        let start = Instant::now();

        let t_vec = dvec_zint_to_vec_int(&t);
        let t_vec_hash = hash_c(&t_vec);
        // let mut shares = Vec::new();
        // for (pk, id) in ids {
        //
        //     let x = generate_x_vectors(ZqInt::new(id as ZqMod, q), ell, r);
        //     let u = generate_polynomial_evaluation(f.clone(), &x, r, ell, ell, kappa, n, q);
        //
        //     let mut v0 = generate_v(f.clone(), &x, r, ell, ell, 1, kappa, n, q);
        //     let c = generate_fiat_shamir_challenge_matrix(&t, &u, &x, &s_vectors[0], &v0, r, kappa ,q);
        //     let mut proof: Vec<ProofUnit> = Vec::new();
        //     proof.push(ProofUnit::new(s_vectors[0].clone(),v0.clone()));
        //     generate_proof(
        //         &mut proof,
        //         &mut x.clone(),
        //         &mut f.clone(),
        //         &mut s_vectors.clone(),
        //         &mut s_vectors[0].clone(),
        //         &mut v0,
        //         c,
        //         ell,
        //         1,
        //         r,
        //         n,
        //         kappa,
        //         q,
        //         log_q
        //     );
        //     let share = Share{
        //         c: Digest([0u8;32]),
        //         y_k: vec![],
        //         proof: vec![],
        //         n,
        //         epoch,
        //     };
        //     shares.push((share,pk));
        // }

        let chunk_size = ids.len() / 10 + 1;
        let mut shares: Vec<_> = ids
            .chunks(chunk_size)
            .par_bridge()
            .flat_map(|chunk| {
                chunk.iter().map(|&(pk, id)| {
                    let x = generate_x_vectors(ZqInt::new(id as ZqMod, q), ell, r);
                    let u = generate_polynomial_evaluation(f.clone(), &x, r, ell, ell, kappa, n, q);

                    let mut v0 = generate_v(f.clone(), &x, r, ell, ell, 1, kappa, n, q);
                    let c = generate_fiat_shamir_challenge_matrix(&t, &u, &x, &s_vectors[0], &v0, r, kappa, q);
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
                        log_q
                    );
                    let share = Share {
                        t: t_vec.clone(),
                        c: t_vec_hash.clone(),
                        y_k: dvec_zint_to_vec_int(&u),
                        merkle_proofs: Vec::default(),
                        eval_proof: proof_unit_to_vec(&proof),
                        epoch,
                    };
                    (share, pk)
                }).collect::<Vec<_>>() // 在每个块内串行处理
            })
            .collect();

        println!("分片证明时间: {:?}",start.elapsed());
        let (roots, proofs) = generate_merkle_proofs(&shares);
        for (share, proof) in shares.iter_mut().zip(proofs.into_iter()) {
            // proof 是 (usize, Vec<Vec<u8>>)，我们需要第二个元素
            share.0.merkle_proofs = proof.1;
        }
        (Shares(shares), roots)
    }
}

fn dvec_zint_to_vec_int(v: &DVector<ZqInt>) -> Vec<ZqMod> {
    let res: Vec<_> = v.iter().map(|&z| z.residue()).collect();
    res
}
fn proof_unit_to_vec(v: &Vec<ProofUnit>) -> Vec<(Vec<ZqMod>, Vec<ZqMod>)> {
    let res: Vec<_> = v.iter().map(|p| p.to_residue_vecs()).collect();
    res
}
fn generate_merkle_proofs(shares: &Vec<(Share,PublicKey)>) -> (Vec<Digest>, Vec<(usize, Vec<Vec<u8>>)>){
    let share_vec: Vec<_> = shares.iter().map(|s| &s.0).collect();
    let mut roots = Vec::new();
    let mut proofs = Vec::new();
    for idx in 0..share_vec[0].y_k.len(){
        let mut layer = Vec::new();
        for share in share_vec.iter(){
            layer.push(share.y_k[idx]);
        }
        let leaves = hash_u128_vec_to_bytes_vec(layer);
        let (root,merkle_proof) = match generate_merkle_tree(leaves) {
            Ok(res) => res,
            Err(_) => { error!("failed to generate merkle tree of layer {}", idx);
            continue; }
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
            hasher.update(&input.to_be_bytes()); // 将 u128 转为大端字节
            hasher.finalize().to_vec() // 输出 32 字节 Vec<u8>
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
    // 如果矩阵为空，直接返回空结果
    if matrix.is_empty() || matrix[0].is_empty() {
        return Vec::new();
    }

    // 获取矩阵的维度
    let b = matrix.len(); // 行数
    let n = matrix[0].len(); // 列数

    // 验证矩阵是否合法（每行长度一致，且 usize 是 1 到 n）
    for row in &matrix {
        if row.len() != n {
            panic!("Invalid matrix: rows have different lengths");
        }
        for (j, &(idx, _)) in row.iter().enumerate() {
            if idx != j + 1 {
                panic!("Invalid matrix: usize values must be 1 to n");
            }
        }
    }

    // 初始化转置结果：n 个 (usize, Vec<Vec<u8>>)
    let mut result = Vec::with_capacity(n);
    for j in 0..n {
        // 每个元素是 (usize, Vec<Vec<u8>>)，usize 是 1 到 n
        let mut column = Vec::with_capacity(b);
        // 遍历原始矩阵的每一行（共 B 行）
        for i in 0..b {
            // 获取原始矩阵 (i, j) 位置的 Vec<u8>
            column.push(matrix[i][j].1.clone());
        }
        result.push((j + 1, column));
    }

    // 按 usize 从低到高排序
    result.sort_by(|a, b| a.0.cmp(&b.0));

    result
}