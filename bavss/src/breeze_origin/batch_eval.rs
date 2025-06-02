use curve25519_dalek::traits::Identity;
use curve25519_dalek::{ Scalar};
use curve25519_dalek::ristretto::RistrettoPoint;
use rand::Rng;
use rs_merkle::algorithms::Sha256;
use rs_merkle::Hasher;
use std::ops::Mul;
use crypto::Digest as CryptoDigest;
use model::breeze_universal::{CommonReferenceString};
use crate::breeze_origin::merkletree::{generate_merkle_tree, verify_merkle_proof};
use crate::breeze_origin::utils::{transpose};
use crate::breeze_structs::{GroupParameters, IProofUnit, PhiElement};
use crate::merkletree::{hash_merkle1, hash_merkle2};

pub fn batch_eval(
    crs: &CommonReferenceString,
    s: &Vec<Vec<Scalar>>,
    y: &Vec<Vec<Scalar>>,
    s_hat: &Vec<RistrettoPoint>,
    t: usize,
    _n: usize
)->(Vec<Vec<Scalar>>,Vec<PhiElement>) {
    let (d, d_hat) = generate_blind_mask(crs, t);
    let (v, v_d) = generate_eval_matrices(&d, s, y);
    let (gamma, sigma) = generate_sigma(s, s_hat);
    let mut sigma_plus_d: Vec<Scalar> = sigma.iter().zip(d.iter()).map(|(a, b)| a + b).collect();
    let sigma_plus_d_hat = batch_commit_for_sigma_plus_d(crs, &sigma_plus_d);
    let v_quota = generate_v_plus_vd_eval(&gamma, &v_d, &v);
    let inner_proof_units =  generate_inner_product_proof(crs, &sigma_plus_d_hat, &v_quota, &mut sigma_plus_d, y, t);
    let phi_tmp: Vec<Vec<IProofUnit>> = transpose(inner_proof_units);

    // Create phi
    let mut phi: Vec<PhiElement> = Vec::new();

    for i in 0..phi_tmp.len() {
        let phi_element = PhiElement {
            proofs: phi_tmp[i].clone(), // Clone or move depending on needs
            d_hat,                 // Assuming D_hat is in scope
            v_d_i: v_d[i],                  // Assuming V_D is in scope
        };
        phi.push(phi_element);
    }

    (v, phi)
}

pub fn batch_verify_eval(crs: &CommonReferenceString, s_hat: &Vec<RistrettoPoint>, v: Vec<Scalar>, mut y: Vec<Scalar>, phi_element: PhiElement, t: usize, n: usize) -> bool{
    let d_hat = phi_element.d_hat;
    let v_d = phi_element.v_d_i;
    let gamma = generate_random_number_by_fiat_shamir(s_hat);
    let mut v_quota = Scalar::ZERO;
    for i in 0..v.len(){
        v_quota += gamma[i]* v[i];
    }
    v_quota += v_d;
    let mut sigma_plus_d_hat = RistrettoPoint::identity();
    for i in 0..gamma.len(){
        sigma_plus_d_hat += s_hat[i] * &gamma[i];
    }
    sigma_plus_d_hat += d_hat;

    let z = generate_statement_challenge_for_verifier(&sigma_plus_d_hat,&v_quota,&y);
    let scalar_term = z * v_quota;
    let h_term = crs.h * scalar_term;
    let sigma_plus_d_hat_quota = sigma_plus_d_hat + h_term;
    let h_z = crs.h * z;
    let mut crs_for_inner_proof = CommonReferenceString {
        g: crs.g.clone(),
        h: h_z,
    };

    inner_product_verify_func(&mut crs_for_inner_proof, sigma_plus_d_hat_quota, &mut y, t+1, phi_element.proofs, n)
}







fn generate_blind_mask(crs: &CommonReferenceString, t: usize) -> (Vec<Scalar>, RistrettoPoint) {
    let mut rng = rand::thread_rng();
    let mut d: Vec<Scalar> = Vec::with_capacity(t + 1);
    for _ in 0..t + 1 {
        let random_val = rng.gen_range(0..(t+1) as u64);
        d.push(Scalar::from(random_val));
    }

    let mut d_hat = RistrettoPoint::identity();
    for i in 0..t + 1 {
        let term = crs.g[i] * d[i];
        d_hat = d_hat + term;
    }

    (d, d_hat)
}

fn generate_eval_matrices(
    d: &Vec<Scalar>,      // t+1 维向量
    s: &Vec<Vec<Scalar>>, // B × (t+1) 矩阵
    y: &Vec<Vec<Scalar>>, // (t+1) × n 矩阵
) -> (Vec<Vec<Scalar>>, Vec<Scalar>) {
    let t_plus_1 = d.len();
    let b = s.len();

    assert_eq!(
        s[0].len(),
        t_plus_1,
        "s matrix column dimension must match d"
    );
    assert_eq!(y.len(), t_plus_1, "y matrix row dimension must match d");

    // 计算 V 矩阵 (B × n)
    let mut v: Vec<Vec<Scalar>> = Vec::with_capacity(b);
    for i in 0..b {
        let v_i = inner_product(&s[i], &y);
        v.push(v_i);
    }

    // 计算 V_D 向量 (n 维)
    let v_d = inner_product(&d, &y);

    (v, v_d)
}

// 计算一个行向量和一个矩阵的内积，返回一个 n 维向量
fn inner_product(
    a: &[Scalar],      // 长度为 t+1 的行向量
    b: &[Vec<Scalar>], // (t+1) × n 矩阵
) -> Vec<Scalar> {
    assert_eq!(a.len(), b.len(), "Inner product dimensions must match");
    let n = b[0].len(); // b 的列数
    let mut result = vec![Scalar::ZERO; n];

    for j in 0..n {
        let mut sum = Scalar::ZERO;
        for i in 0..a.len() {
            sum = sum + a[i] * b[i][j];
        }
        result[j] = sum;
    }

    result
}

fn gen_hash(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }
    let hasher = Sha256::hash(input);
    hasher.to_vec()
}

fn generate_random_number_by_fiat_shamir(s_hat: &Vec<RistrettoPoint>) -> Vec<Scalar> {
    let mut gamma = vec![Scalar::ZERO; s_hat.len()];

    // 将 S_hat 序列化为字节
    let mut s_hat_bytes = Vec::new();
    for point in s_hat {
        let compressed = point.compress();
        s_hat_bytes.extend_from_slice(compressed.as_bytes());
    }

    // 计算 gamma[i] = H(S_hat || i)
    for i in 0..s_hat.len() {
        let mut input = s_hat_bytes.clone();
        input.push(i as u8); // 追加索引 i
        let gamma_bytes = gen_hash(&input);
        // Scalar::from_bytes_mod_order 接受 32 字节输入
        gamma[i] = Scalar::from_bytes_mod_order(
            gamma_bytes
                .try_into()
                .expect("Hash output must be 32 bytes"),
        );
    }

    gamma
}

fn generate_sigma(
    s: &Vec<Vec<Scalar>>,        // B × (t+1) 矩阵
    s_hat: &Vec<RistrettoPoint>, // 长度为 B 的向量
) -> (Vec<Scalar>, Vec<Scalar>) {
    // 初始化 sigma 向量
    let t_plus_1 = s[0].len(); // s 的列数
    let mut sigma = vec![Scalar::ZERO; t_plus_1];

    // 生成 gamma
    let gamma = generate_random_number_by_fiat_shamir(s_hat);

    // 检查维度
    assert_eq!(s.len(), gamma.len(), "s rows must match gamma length");

    // 计算 sigma[j] = Σ(gamma[i] * s[i][j])
    for i in 0..t_plus_1 {
        for j in 0..s.len() {
            sigma[i] = sigma[i] + gamma[j] * s[j][i];
        }
    }

    (gamma, sigma)
}

fn batch_commit_for_sigma_plus_d(
    crs: &CommonReferenceString, // GVec，长度至少为 len(s_and_d)
    sigma_plus_d: &Vec<Scalar>,  // 标量向量
) -> RistrettoPoint {
    let mut res = RistrettoPoint::identity(); // 零点，对应 Null()

    // 检查维度
    assert_eq!(
        crs.g.len(),
        sigma_plus_d.len(),
        "g and s_and_d lengths must match"
    );

    // 计算 res = Σ(g[i] * s_and_d[i])
    for i in 0..sigma_plus_d.len() {
        let term = crs.g[i] * sigma_plus_d[i];
        res = res + term;
    }

    res
}

fn generate_v_plus_vd_eval(
    gamma: &Vec<Scalar>,  // 长度为 B 的向量
    v_d: &Vec<Scalar>,    // 长度为 n 的向量
    v: &Vec<Vec<Scalar>>, // B × n 矩阵
) -> Vec<Scalar> {
    // 初始化 V_quota 为零向量，长度与 V_D 相同
    let mut v_quota = vec![Scalar::ZERO; v_d.len()];

    // 检查维度
    assert_eq!(v.len(), gamma.len(), "V rows must match gamma length");
    assert_eq!(v[0].len(), v_d.len(), "V columns must match V_D length");

    // 第一步：计算 V_quota[j] = Σ(gamma[i] * V[i][j])
    for i in 0..v.len() {
        for j in 0..v[i].len() {
            v_quota[j] = v_quota[j] + gamma[i] * v[i][j];
        }
    }

    // 第二步：V_quota[i] = V_quota[i] + V_D[i]
    for i in 0..v_quota.len() {
        v_quota[i] = v_quota[i] + v_d[i];
    }

    v_quota
}

fn generate_inner_product_proof(
    crs: &CommonReferenceString,
    sigma_plus_d_hat: &RistrettoPoint,
    v_quota: &Vec<Scalar>,
    sigma_plus_d: &mut Vec<Scalar>,
    y: &Vec<Vec<Scalar>>,
    t: usize,
) -> Vec<Vec<IProofUnit>> {
    // 初始化 SD_hat_quota
    let mut sigma_plus_d_hat_quota = vec![RistrettoPoint::identity(); v_quota.len()];

    // 计算 z
    let z = generate_statement_challenges(sigma_plus_d_hat, v_quota, y);
    // 计算 SD_hat_quota[i] = SD_hat + (z[i] * V_quota[i]) * H
    for i in 0..z.len() {
        let scalar_term = z[i] * v_quota[i];
        let h_term = crs.h.mul(scalar_term);
        sigma_plus_d_hat_quota[i] = sigma_plus_d_hat + h_term;
    }

    // 计算 h_z[i] = z[i] * H
    let mut h_z = vec![RistrettoPoint::identity(); z.len()];
    for i in 0..z.len() {
        h_z[i] = crs.h.mul(z[i]);
    }
    let mut group = GroupParameters {
        g_vec: crs.g.clone(),
        h_vec: h_z,
    };
    let mut phi: Vec<Vec<IProofUnit>> = Vec::new();
    let inner_proof_units = inner_product_proof_func(&mut phi, &mut group, &mut sigma_plus_d_hat_quota, &mut y.clone(), t+1, sigma_plus_d);

    inner_proof_units.clone()
}

fn generate_statement_challenge_for_verifier(
    sigma_plus_d_hat: &RistrettoPoint, // SD_hat 点
    v_quota: &Scalar,             // 长度为 n 的向量
    y: &Vec<Scalar>,              // (t+1) × n 矩阵
)-> Scalar {
    let compressed = sigma_plus_d_hat.compress();
    let sigma_plus_d_hat_bytes = compressed.as_bytes();
    let mut y_bytes = Vec::new();
    for i in 0..y.len() {
        let scalar_bytes = y[i].to_bytes();
        y_bytes.extend_from_slice(&scalar_bytes);
    }
    let mut stmt = Vec::new();
    stmt.extend_from_slice(sigma_plus_d_hat_bytes);
    stmt.extend_from_slice(&y_bytes);
    let v_quota_bytes = v_quota.to_bytes();
    stmt.extend_from_slice(&v_quota_bytes);

    let hash = gen_hash(&stmt);
    Scalar::from_bytes_mod_order(hash.try_into().expect("Hash output must be 32 bytes"))
}
fn generate_statement_challenges(
    sigma_plus_d_hat: &RistrettoPoint, // SD_hat 点
    v_quota: &Vec<Scalar>,             // 长度为 n 的向量
    y: &Vec<Vec<Scalar>>,              // (t+1) × n 矩阵
) -> Vec<Scalar> {
    let mut z = vec![Scalar::ZERO; v_quota.len()];

    // 序列化 SD_hat
    let compressed = sigma_plus_d_hat.compress();
    let sigma_plus_d_hat_bytes = compressed.as_bytes();

    // 序列化 y，按列组织
    let n = y[0].len();
    let mut y_bytes = vec![Vec::new(); n];
    for i in 0..y.len() {
        for j in 0..n {
            let scalar_bytes = y[i][j].to_bytes(); // Scalar 的 32 字节表示
            y_bytes[j].extend_from_slice(&scalar_bytes);
        }
    }

    // 计算 z[i] = H(SD_hat || y[:,i] || V_quota[i])
    for i in 0..z.len() {
        let mut stmt = Vec::new();
        stmt.extend_from_slice(sigma_plus_d_hat_bytes);
        stmt.extend_from_slice(&y_bytes[i]);
        let v_quota_bytes = v_quota[i].to_bytes();
        stmt.extend_from_slice(&v_quota_bytes);

        let hash = gen_hash(&stmt);
        z[i] = Scalar::from_bytes_mod_order(hash.try_into().expect("Hash output must be 32 bytes"));
    }
    z
}

fn inner_product_proof_func<'a>(
    phi: &'a mut Vec<Vec<IProofUnit>>,
    group: &mut GroupParameters,
    s_d_hat: &mut Vec<RistrettoPoint>,
    y: &mut Vec<Vec<Scalar>>,
    mut p: usize,
    a: &mut Vec<Scalar>,
) -> &'a Vec<Vec<IProofUnit>> {
    assert_eq!(
        group.g_vec.len(),
        p,
        "Vectors must have the same length"
    );
    assert_eq!(
        a.len(),
        p,
        "Vectors must have the same length"
    );
    assert_eq!(y.len(), p, "Vectors must have the same length");
    assert_eq!(
        group.h_vec.len(),
        y[0].len(),
        "Vectors must have the same length"
    );
    assert_eq!(
        group.h_vec.len(),
        s_d_hat.len(),
        "Vectors must have the same length"
    );
    let mut flag = 0;

    let mut phi_this: Vec<IProofUnit> = vec![
        IProofUnit {
            a_only: Scalar::ZERO,
            l: RistrettoPoint::identity(),
            r: RistrettoPoint::identity(),
            z: CryptoDigest([0;32]),
            b_i: (0,Vec::new()),
            a_tilde: Scalar::ZERO,
        };
        s_d_hat.len()
    ];
    let z: CryptoDigest;

    #[allow(unused_assignments)]
    let mut branches:Vec<(usize, Vec<u8>)> = Vec::new();

    if p == 1 {
        for idx in 0..s_d_hat.len(){
            phi_this[idx].a_only = a[0];
        }
        phi.push(phi_this);
        return phi;
    }
    let mut a_tilde = Scalar::ZERO;
    if p % 2 == 1 {
        flag = 1;
        a_tilde = -a[p-1];  // Negate the scalar
        a.truncate(p-1);     // Slice the array up to p-1

        let temp1 = group.g_vec[p-1] * a_tilde;  // g_p * a_tilde

        for idx in 0..s_d_hat.len() {
            let temp2 = a_tilde * y[p-1][idx];         // a_tilde * y[p,i]
            let temp3 = group.h_vec[idx] * temp2;      // h * (z_i * a_tilde * y[p,i])
            let temp4 = temp1 + temp3;                 // g_p**a_tilde * h**(z_i * a_tilde * y[p,i])
            s_d_hat[idx] = s_d_hat[idx] + temp4;
            phi_this[idx].a_tilde = a_tilde;          // Add a_tilde to the proof
        }

        y.truncate(p-1);      // Slice y up to p-1
        group.g_vec.truncate(p-1);   // Slice g_vec up to p-1
        p -= 1;
    }
    p = p / 2;
    let (a_l, a_r) = (&a[..p], &a[p..]);
    let (y_l, y_r) = (&y[..p], &y[p..]);
    let (mut g_l, mut g_r) = (group.g_vec[..p].to_vec(), group.g_vec[p..].to_vec());

    let c_l = inner_product(&a_l, &y_r);
    let c_r = inner_product(&a_r, &y_l);

    let mut l: Vec<RistrettoPoint> = vec![RistrettoPoint::identity(); group.h_vec.len()];
    let mut r: Vec<RistrettoPoint> = vec![RistrettoPoint::identity(); group.h_vec.len()];

    let mut l_temp = RistrettoPoint::identity();
    let mut r_temp = RistrettoPoint::identity();
    // First loop: Compute l_temp and r_temp
    for idx in 0..g_r.len() {
        l_temp = l_temp + (g_r[idx] * a_l[idx]);  // g_r ^ a_l
        r_temp = r_temp + (g_l[idx] * a_r[idx]);  // g_l ^ a_r
    }

    // Second loop: Compute l and r
    for idx in 0..group.h_vec.len() {
        l[idx] = l_temp + (group.h_vec[idx] * c_l[idx]);
        r[idx] = r_temp + (group.h_vec[idx] * c_r[idx]);
    }

    // Transpose y into y_
    let mut y_: Vec<Vec<Scalar>> = vec![Vec::new(); y[0].len()];
    for i in 0..y.len() {
        for j in 0..y[0].len() {
            y_[j].push(y[i][j]);
        }
    }

    if flag == 1 {
        let mut leafs: Vec<Vec<u8>> = vec![Vec::new(); group.h_vec.len()];
        for idx in 0..group.h_vec.len() {
            leafs[idx] = hash_merkle1(
                &group.g_vec,
                p,
                group.h_vec[idx],
                &y_[idx],
                s_d_hat[idx],
                l[idx],
                r[idx],
                a_tilde
            );
        }
        (z, branches) = generate_merkle_tree(leafs)
            .expect("Failed to generate Merkle tree");
    } else {
        let mut leafs: Vec<Vec<u8>> = vec![Vec::new(); group.h_vec.len()];
        for idx in 0..group.h_vec.len() {
            leafs[idx] = hash_merkle2(
                &group.g_vec,
                p,
                group.h_vec[idx],
                &y_[idx],
                s_d_hat[idx],
                l[idx],
                r[idx]
            );
        }
        (z, branches) = generate_merkle_tree(leafs)
            .expect("Failed to generate Merkle tree");
    }

    // Convert z bytes to scalar and compute related values
    let z_scalar = Scalar::from_bytes_mod_order(z.0);           // z
    let z_scalar_inv = z_scalar.invert();                     // 1/z
    let z2 = z_scalar * z_scalar;                            // z**2
    let z2_inv = z_scalar_inv * z_scalar_inv;                // 1/z**2

    // Compute A_prime, l2, and r2
    let mut s_d_hat_prime: Vec<RistrettoPoint> = vec![RistrettoPoint::identity(); s_d_hat.len()];
    let mut l2: Vec<RistrettoPoint> = vec![RistrettoPoint::identity(); s_d_hat.len()];      // l**{z**2}
    let mut r2: Vec<RistrettoPoint> = vec![RistrettoPoint::identity(); s_d_hat.len()];      // r**{1/z**2}

    for idx in 0..s_d_hat.len() {
        phi_this[idx].a_only = Scalar::ZERO;
        phi_this[idx].l = l[idx];
        phi_this[idx].r = r[idx];
        phi_this[idx].z = z;
        phi_this[idx].b_i = branches[idx].clone();
        l2[idx] = l[idx] * z2;                  // l**{z**2}
        r2[idx] = r[idx] * z2_inv;              // r**{1/z**2}
        s_d_hat_prime[idx] = l2[idx] + s_d_hat[idx] + r2[idx];  // l**{z**2} * A * r**{1/z**2}
    }

    let mut g_prime: Vec<RistrettoPoint> = vec![RistrettoPoint::identity(); g_l.len()];
    let mut y_prime: Vec<Vec<Scalar>> = vec![vec![Scalar::ZERO; y_l[0].len()]; y_l.len()];
    let mut a_prime: Vec<Scalar> = vec![Scalar::ZERO; y_l.len()];

    // Compute transformations
    for idx in 0..g_l.len() {
        g_l[idx] = g_l[idx] * z_scalar_inv;     // g_l**{1/z}
        g_r[idx] = g_r[idx] * z_scalar;         // g_r**{z}
        g_prime[idx] = g_l[idx] + g_r[idx];     // g_l**{1/z} * g_r**{z}

        // Compute a_prime
        a_prime[idx] = z_scalar * a_l[idx];                     // z * a_l
        a_prime[idx] = a_prime[idx] + (z_scalar_inv * a_r[idx]); // z * a_l + 1/z * a_r

        // Compute y_prime
        for j in 0..y_l[0].len() {
            y_prime[idx][j] = z_scalar_inv * y_l[idx][j];                    // 1/z * y_l
            y_prime[idx][j] = y_prime[idx][j] + (z_scalar * y_r[idx][j]);     // 1/z * y_l + z * y_r
        }
    }
    group.g_vec = g_prime;
    phi.push(phi_this);
    let res = inner_product_proof_func(phi, group, &mut s_d_hat_prime, &mut y_prime, p, &mut a_prime);
    res
}


fn inner_product_verify_func(
    group: &mut CommonReferenceString,
    mut s_d_hat: RistrettoPoint,
    y: &mut Vec<Scalar>,
    mut p: usize,
    mut phi: Vec<IProofUnit>,
    n: usize
) -> bool {
    let leaf: Vec<u8>;
    if p == 1{
        let a = phi[0].a_only;
        let temp1 = a * group.g[0];
        let temp2 = a * y[0] * group.h;
        let temp3 = temp1 + temp2;
        return s_d_hat == temp3;
    }
    if p % 2 == 1{
        let a_tilde = phi[0].a_tilde;
        let temp1 = a_tilde * group.g[p-1];
        let temp2 = a_tilde * y[p-1];
        let temp3 = temp2 * group.h;
        let temp4 = temp1 + temp3;
        s_d_hat += temp4;
        y.truncate(p-1);
        group.g.truncate(p-1);
        p -= 1;
        p = p / 2;
        let l = phi[0].l;
        let r = phi[0].r;
        leaf = hash_merkle1(&group.g, p, group.h, y, s_d_hat, l, r, a_tilde);
    } else{
        let l = phi[0].l;
        let r = phi[0].r;
        p = p / 2;
        leaf = hash_merkle2(&group.g, p, group.h, y, s_d_hat, l, r);
    }
    match verify_merkle_proof(&leaf, phi[0].b_i.clone(), phi[0].z, n) {
        Ok(result) =>{
            if !result{
                return false;
            }
        }
        Err(_) => {panic!("failed to verify merkle proof");}
    }
    let (mut g_l, mut g_r) = (group.g[..p].to_vec(), group.g[p..].to_vec());
    let z_scalar = Scalar::from_bytes_mod_order(phi[0].z.0);           // z
    let z_scalar_inv = z_scalar.invert();                     // 1/z
    let z2 = z_scalar * z_scalar;                            // z**2
    let z2_inv = z_scalar_inv * z_scalar_inv;                // 1/z**2

    let l2 = z2* phi[0].l;      // L**{z**2}
    let r2 = z2_inv * phi[0].r;      // R**{1/z**2}

    let a_prime = l2 + s_d_hat + r2;

    let (y_l, y_r) = (&y[..p], &y[p..]);

    let mut g_prime: Vec<RistrettoPoint> = vec![RistrettoPoint::identity(); g_l.len()];
    let mut y_prime: Vec<Scalar> = vec![Scalar::ZERO; y_l.len()];

    for idx in 0..g_l.len() {
        g_l[idx] = g_l[idx] * z_scalar_inv;     // g_L**{1/z}
        g_r[idx] = g_r[idx] * z_scalar;         // g_R**{z}
        g_prime[idx] = g_l[idx] + g_r[idx];     // g_L**{1/z} * g_R**{z}

        // Compute y_prime
        y_prime[idx] = z_scalar_inv * y_l[idx];                // 1/z * y_L
        y_prime[idx] = y_prime[idx] + (z_scalar * y_r[idx]);   // 1/z * y_L + z * y_R
    }

    // Update group and phi
    group.g = g_prime;
    phi = phi[1..].to_vec(); // Slice from index 1 to end and convert to owned Vec

    inner_product_verify_func(group, a_prime, &mut y_prime, p, phi, n)
}