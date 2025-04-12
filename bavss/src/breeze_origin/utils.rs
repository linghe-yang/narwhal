use curve25519_dalek::{RistrettoPoint, Scalar};
use sha2::{Digest, Sha256};
pub fn transpose<T: Clone>(matrix: Vec<Vec<T>>) -> Vec<Vec<T>> {
    // 检查矩阵是否为空或不规则
    if matrix.is_empty() {
        return Vec::new();
    }

    let rows = matrix.len();
    let cols = matrix[0].len();

    // 检查矩阵是否规则（每行长度相同）
    if !matrix.iter().all(|row| row.len() == cols) {
        panic!("Matrix must have consistent row lengths");
    }

    // 创建转置矩阵
    let mut result = Vec::with_capacity(cols);
    for j in 0..cols {
        let mut new_row = Vec::with_capacity(rows);
        for i in 0..rows {
            new_row.push(matrix[i][j].clone());
        }
        result.push(new_row);
    }

    result
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
    let mut hasher = Sha256::new();
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
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sym = hasher.finalize().to_vec();
    sym
}