use crypto::Digest;
use rs_merkle::{algorithms::Sha256, Hasher, MerkleProof, MerkleTree};
use std::error::Error;

pub fn generate_merkle_tree(input: Vec<Vec<u8>>) -> Result<(Digest, Vec<(usize, Vec<u8>)>), Box<dyn Error>> {
    // 将输入转换为 SHA-256 哈希值的叶子节点
    let leaves: Vec<[u8; 32]> = input
        .into_iter()
        .map(|data| Sha256::hash(&data))
        .collect();
    // 创建 Merkle 树
    let tree = MerkleTree::<Sha256>::from_leaves(&leaves);
    let root = tree.root().ok_or("Failed to get Merkle root")?;

    let mut proofs = Vec::new();
    for i in 1..=leaves.len() {
        // 为当前 leaf 索引生成 proof
        let indices_to_prove = vec![i];
        let merkle_proof = tree.proof(&indices_to_prove);
        let proof_bytes = merkle_proof.to_bytes();
        proofs.push((i,proof_bytes));

    }

    Ok((Digest(root), proofs))
}
pub fn verify_merkle_proof(
    leaf: &Vec<u8>,
    proof_tuple: (usize, Vec<u8>),
    root: Digest,
    total_leaves_count: usize,
) -> Result<bool, Box<dyn Error>> {
    let root = root.0;
    let leave_to_prove = [Sha256::hash(leaf)];
    let proof = MerkleProof::<Sha256>::try_from(proof_tuple.1)?;
    Ok(proof.verify(root, &vec![proof_tuple.0], &leave_to_prove, total_leaves_count))
}
