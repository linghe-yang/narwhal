use rs_merkle::{MerkleTree, algorithms::Sha256, Hasher, MerkleProof};
use sha2::{Digest, Sha256 as S256};
use std::error::Error;
use curve25519_dalek::{RistrettoPoint, Scalar};
use crypto::{Digest as CryptoDigest};

pub fn generate_merkle_tree(input: Vec<Vec<u8>>) -> Result<(CryptoDigest, Vec<(usize, Vec<u8>)>), Box<dyn Error>> {
    // 将输入转换为 SHA-256 哈希值的叶子节点
    let leaves: Vec<[u8; 32]> = input
        .into_iter()
        .map(|data| Sha256::hash(&data))
        .collect();

    // 创建 Merkle 树
    let tree = MerkleTree::<Sha256>::from_leaves(&leaves);
    let root = tree.root().ok_or("Failed to get Merkle root")?;

    let mut proofs = Vec::new();
    for i in 0..leaves.len() {
        // 为当前 leaf 索引生成 proof
        let indices_to_prove = vec![i];
        let merkle_proof = tree.proof(&indices_to_prove);
        let proof_bytes = merkle_proof.to_bytes();
        proofs.push((i,proof_bytes));

    }

    Ok((CryptoDigest(root), proofs))
}
pub fn verify_merkle_proof(
    leaf: &Vec<u8>,           // 要验证的叶子节点原始数据
    proof_tuple: (usize, Vec<u8>),     // Merkle Proof（一系列哈希值）
    root: CryptoDigest,           // Merkle 树的根哈希
    total_leaves_count: usize,
) -> Result<bool, Box<dyn Error>> {
    let root = root.0;
    let leave_to_prove = [Sha256::hash(leaf)];
    let proof = MerkleProof::<Sha256>::try_from(proof_tuple.1)?;
    Ok(proof.verify(root, &vec![proof_tuple.0], &leave_to_prove, total_leaves_count))
}

pub fn hash_merkle1(
    g: &[RistrettoPoint],
    p: usize,
    h: RistrettoPoint,
    y: &[Scalar],
    a_i: RistrettoPoint,
    l_i: RistrettoPoint,
    r_i: RistrettoPoint,
    a_til: Scalar
) -> Vec<u8> {
    // Initialize bytes vector
    let mut bytes = Vec::new();

    // Serialize g and y to bytes
    for i in 0..g.len() {
        let g_bytes = g[i].compress().to_bytes(); // RistrettoPoint to bytes
        let y_bytes = y[i].to_bytes();           // Scalar to bytes
        bytes.extend_from_slice(&g_bytes);
        bytes.extend_from_slice(&y_bytes);
    }

    // Serialize p to bytes
    let p_bytes = p.to_be_bytes(); // Convert integer to big-endian bytes
    bytes.extend_from_slice(&p_bytes);

    // Serialize h, A_i, L_i, R_i, and a_til to bytes
    let h_bytes = h.compress().to_bytes();
    let a_bytes = a_i.compress().to_bytes();
    let l_bytes = l_i.compress().to_bytes();
    let r_bytes = r_i.compress().to_bytes();
    let a_til_bytes = a_til.to_bytes();

    bytes.extend_from_slice(&h_bytes);
    bytes.extend_from_slice(&a_bytes);
    bytes.extend_from_slice(&l_bytes);
    bytes.extend_from_slice(&r_bytes);
    bytes.extend_from_slice(&a_til_bytes);

    // Generate hash (assuming SHA-256 as an example)
    let mut hasher = S256::new();
    hasher.update(&bytes);
    let sym = hasher.finalize().to_vec();

    sym
}

pub fn hash_merkle2(
    g: &[RistrettoPoint],
    p: usize,
    h: RistrettoPoint,
    y: &[Scalar],
    a_i: RistrettoPoint,
    l_i: RistrettoPoint,
    r_i: RistrettoPoint
) -> Vec<u8> {
    // Initialize bytes vector
    let mut bytes = Vec::new();

    // Serialize g and y to bytes
    for i in 0..g.len() {
        let g_bytes = g[i].compress().to_bytes(); // RistrettoPoint to bytes
        let y_bytes = y[i].to_bytes();           // Scalar to bytes
        bytes.extend_from_slice(&g_bytes);
        bytes.extend_from_slice(&y_bytes);
    }

    // Serialize p to bytes
    let p_bytes = p.to_be_bytes(); // Convert integer to big-endian bytes
    bytes.extend_from_slice(&p_bytes);

    // Serialize h, A_i, L_i, R_i, and a_til to bytes
    let h_bytes = h.compress().to_bytes();
    let a_bytes = a_i.compress().to_bytes();
    let l_bytes = l_i.compress().to_bytes();
    let r_bytes = r_i.compress().to_bytes();

    bytes.extend_from_slice(&h_bytes);
    bytes.extend_from_slice(&a_bytes);
    bytes.extend_from_slice(&l_bytes);
    bytes.extend_from_slice(&r_bytes);

    // Generate hash (assuming SHA-256 as an example)
    let mut hasher = S256::new();
    hasher.update(&bytes);
    let sym = hasher.finalize().to_vec();

    sym
}