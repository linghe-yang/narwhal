use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::iter::IndexedParallelIterator;
use nalgebra::{DMatrix, DVector};
use rayon::iter::IntoParallelRefMutIterator;
use sha2::{Digest, Sha256};
use model::types_and_const::ZqMod;
use crate::breeze_pq::polynomial::Polynomial;
use crate::breeze_pq::zq_int::ZqInt;
use crate::breeze_structs::ProofUnit;

pub fn generate_f_vector(
    r: usize,
    ell: usize,
    kappa: usize,
    n: usize,
    modulus: ZqMod,
    polynomials: Vec<Polynomial>,
) -> DVector<ZqInt> {
    assert!(modulus > 0, "Modulus must be positive");
    // 计算 r^(ell+1)
    let r_pow_ell_plus_1 = r.pow((ell + 1) as u32);
    let total_length = r_pow_ell_plus_1 * kappa * n;
    // 检查多项式个数 k
    let k = polynomials.len();
    if k > kappa * n {
        panic!("Number of polynomials ({}) exceeds kappa * n ({})", k, kappa * n);
    }
    // 检查多项式次数一致性并获取 t
    if polynomials.is_empty() {
        panic!("At least one polynomial is required");
    }
    let t = polynomials[0].degree();
    for poly in &polynomials[1..] {
        if poly.degree() != t {
            panic!("All polynomials must have the same degree");
        }
    }
    // 检查 t+1 <= r^(ell+1)
    let coeff_count = t + 1;
    if coeff_count > r_pow_ell_plus_1 {
        panic!(
            "Coefficient count ({}) exceeds r^(ell+1) ({})",
            coeff_count, r_pow_ell_plus_1
        );
    }
    // 构建 f 向量
    let mut coeffs = Vec::with_capacity(total_length);
    // 填充 k 个多项式的系数
    for poly in &polynomials {
        // 填充现有系数
        for i in 0..coeff_count {
            coeffs.push(poly.get_coeff(i));
        }
        // 补 0 至 r^(ell+1)
        for _ in coeff_count..r_pow_ell_plus_1 {
            coeffs.push(ZqInt::new(0, modulus));
        }
    }
    // 补剩余的 0（缺失的多项式部分）
    let filled_length = k * r_pow_ell_plus_1;
    for _ in filled_length..total_length {
        coeffs.push(ZqInt::new(0, modulus));
    }
    DVector::from_vec(coeffs)
}
// pub fn generate_a_matrix(a: &Vec<Vec<ZqMod>>, q: ZqMod) -> DMatrix<ZqInt> {
//     let nrows = a.len();
//     assert!(nrows > 0, "Matrix must have at least one row");
//     let ncols = a[0].len();
//     assert!(ncols > 0, "Matrix must have at least one column");
//     assert!(a.iter().all(|row| row.len() == ncols), "All rows must have the same length");
//     let flat_data: Vec<ZqInt> = a.into_iter()
//         .flat_map(|row| {
//             row.into_iter()
//                 .map(|val| ZqInt::new(*val, q))
//         })
//         .collect();
//     DMatrix::from_vec(nrows, ncols, flat_data)
// }
pub fn generate_t(
    f: &DVector<ZqInt>,
    s_vectors: &mut Vec<Option<DVector<ZqInt>>>,
    r: usize,
    ell: usize,
    kappa: usize,
    n: usize,
    q: ZqMod,
    log_q: usize,
    a: &DMatrix<ZqInt>,
) -> DVector<ZqInt> {
    let mut t = f.clone();
    let zero = ZqInt::new(0, q);
    let one = ZqInt::new(1, q);

    // 从 ell 到 0 迭代
    for i in (0..=ell).rev() {
        let k = r.pow(i as u32) * kappa;
        let l = r.pow(i as u32 + 1) * kappa * n;

        let s = find_low_norm_vector(l, &t, &zero, &one, log_q);

        s_vectors[i] = Some(s.clone());

        t = i_kron_a_dot_s(a, &s, k, &zero, &one);

        assert_eq!(t.len(), r.pow(i as u32) * kappa * n);
        assert_eq!(s.len(), r.pow(i as u32 + 1) * kappa * n * log_q);
    }

    t
}
pub fn find_low_norm_vector(
    m: usize,
    f: &DVector<ZqInt>,
    zero: &ZqInt,
    one: &ZqInt,
    log_q: usize,
) -> DVector<ZqInt> {
    let max_base = 1 << log_q;

    let mut s = Vec::with_capacity(m * log_q);

    // 并行生成 s 的每个 m 块
    let s_chunks: Vec<_> = (0..m).into_par_iter().flat_map(|i| {
        let f_i = f[i].residue();
        if f_i >= max_base {
            panic!("Cannot express f[{}] = {} with log q = {}", i, f_i, log_q);
        }

        let mut s_i = Vec::with_capacity(log_q);
        for j in 0..log_q {
            let base = 1 << j;
            s_i.push(if f_i & base != 0 { *one } else { *zero });
        }
        s_i
    }).collect();

    s.extend(s_chunks);
    DVector::from_vec(s)
}

pub fn generate_x_vectors(x: ZqInt, ell: usize, r: usize) -> Vec<DVector<ZqInt>> {
    let q = x.modulus(); // 从 x 获取模数
    let mut result = Vec::with_capacity(ell + 1); // 预分配 l+1 个向量

    for i in 0..=ell {
        let mut coefficients = Vec::with_capacity(r); // 预分配每个向量的容量
        let power_base = r.pow(i as u32); // r^i
        // 计算 x^(r^i * (j-1))，j 从 1 到 r
        let mut current = ZqInt::new(1, q); // x^0 = 1
        for _ in 1..=r {
            coefficients.push(current);
            // 计算下一个元素：x^(r^i * j) = x^(r^i * (j-1)) * x^(r^i)
            let step = power_mod(x, power_base, q); // x^(r^i)
            current = current * step;
        }

        result.push(DVector::from_vec(coefficients));
    }
    result
}

pub fn generate_polynomial_evaluation(
    f: DVector<ZqInt>,
    x: &Vec<DVector<ZqInt>>,
    r: usize,
    mut ell: usize,
    ell_begin: usize,
    kappa: usize,
    n: usize,
    q: ZqMod
) -> DVector<ZqInt> {
    let x_l = &x[ell_begin - ell];
    let ul = i_kron_vec_dot_vec(x_l, &f, r.pow(ell as u32) * kappa * n, q);
    if ell == 0 {
        return ul;
    }
    ell -= 1;
    generate_polynomial_evaluation(ul, x, r, ell, ell_begin, kappa, n, q)
}
pub fn generate_v(
    f: DVector<ZqInt>,
    x: &Vec<DVector<ZqInt>>,
    r: usize,
    mut ell: usize,
    ell_begin: usize,
    ell_end: usize,
    kappa: usize,
    n: usize,
    q: ZqMod,
) -> DVector<ZqInt> {
    let mut result = f;

    while ell >= ell_end {
        let x_l = &x[ell_begin - ell];
        result = i_kron_vec_dot_vec(
            x_l,
            &result,
            r.pow((ell - ell_end + 1) as u32) * kappa * n,
            q
        );
        if ell == ell_end {
            break;
        }
        ell -= 1;
    }
    result
}
pub fn generate_fiat_shamir_challenge_matrix(
    t: &DVector<ZqInt>,
    u: &DVector<ZqInt>,
    x_vecs: &Vec<DVector<ZqInt>>,
    y: &DVector<ZqInt>,
    v: &DVector<ZqInt>,
    r: usize,
    kappa: usize,
    q: ZqMod
) -> DMatrix<ZqInt> {
    // 计算矩阵大小
    let rows = r * kappa;
    let cols = kappa;
    let total_bits = rows * cols; // 需要的比特数
    let total_bytes = (total_bits + 7) / 8; // 向上取整到字节

    // SHA256 has post-quantum security of 128 bits.
    let mut hasher = Sha256::new();

    // 添加 t
    for coeff in t.iter() {
        hasher.update(&coeff.residue().to_le_bytes());
    }
    // 添加 u
    for coeff in u.iter() {
        hasher.update(&coeff.residue().to_le_bytes());
    }
    // 添加 x_vecs
    for x in x_vecs {
        for coeff in x.iter() {
            hasher.update(&coeff.residue().to_le_bytes());
        }
    }
    // 添加 y
    for coeff in y.iter() {
        hasher.update(&coeff.residue().to_le_bytes());
    }
    // 添加 v
    for coeff in v.iter() {
        hasher.update(&coeff.residue().to_le_bytes());
    }

    // 生成足够长的字节序列
    let mut bytes = Vec::with_capacity(total_bytes);
    let mut counter = 0u32;
    while bytes.len() < total_bytes {
        let mut counter_hasher = hasher.clone();
        counter_hasher.update(&counter.to_le_bytes());
        let hash = counter_hasher.finalize();
        bytes.extend_from_slice(&hash);
        counter += 1;
    }

    // 截取所需字节
    bytes.truncate(total_bytes);

    // 转换为 0-1 矩阵
    let mut data = Vec::with_capacity(total_bits);
    for byte in bytes {
        for bit in 0..8 {
            if data.len() < total_bits {
                let value = (byte >> bit) & 1;
                data.push(ZqInt::new(value as ZqMod, q));
            } else {
                break;
            }
        }
    }

    let res = DMatrix::from_vec(rows, cols, data).transpose();
    res
}

pub fn generate_proof(
    proofs: &mut Vec<ProofUnit>,
    x: &mut Vec<DVector<ZqInt>>,
    fi: &mut DVector<ZqInt>,
    si: &mut Vec<DVector<ZqInt>>,
    yi: &mut DVector<ZqInt>,
    vi: &mut DVector<ZqInt>,
    mut ci_plus_1: DMatrix<ZqInt>,
    ell: usize,
    mut depth: usize,
    r: usize,
    n: usize,
    kappa: usize,
    q: ZqMod,
    log_q: usize,
) {
    proofs.reserve(ell); // 预分配容量以减少动态扩展

    while depth <= ell {
        let ti_plus_1 = c_kron_g_dot_y(&ci_plus_1, yi, n, q, log_q);
        let ui_plus_1 = c_kron_i_dot_v(&ci_plus_1, vi, n, q);

        // 预分配 si_plus_1，长度为 si.len() - 1
        let mut si_plus_1 = Vec::with_capacity(si.len() - 1);

        // 初始化 si_plus_1，每个向量的长度根据公式计算
        for j in 0..si.len() - 1 {
            let output_len = r.pow(j as u32 + 1) * n * log_q * kappa;
            si_plus_1.push(dvector_zeros(output_len, q));
        }

        // 并行计算 si_plus_1[j]
        si_plus_1.par_iter_mut().enumerate().for_each(|(j, s)| {
            *s = c_kron_i_dot_v(&ci_plus_1, &si[j + 1], r.pow(j as u32 + 1) * n * log_q, q);
        });

        // 用 si_plus_1 替换 si
        *si = si_plus_1;
        x.truncate(x.len() - 1);

        *yi = si[0].clone();
        *fi = c_kron_i_dot_v(&ci_plus_1, fi, r.pow((ell - depth + 1) as u32) * n, q);

        if depth == ell {
            proofs.push(ProofUnit::new(yi.clone(), fi.clone()));
            break;
        }

        *vi = generate_v(fi.clone(), x, r, ell, ell, depth + 1, kappa, n, q);
        proofs.push(ProofUnit::new(yi.clone(), vi.clone()));

        ci_plus_1 = generate_fiat_shamir_challenge_matrix(&ti_plus_1, &ui_plus_1, x, yi, vi, r, kappa, q);
        depth += 1;
    }
}
pub fn power_mod(x: ZqInt, n: usize, q: ZqMod) -> ZqInt {
    let mut result = ZqInt::new(1, q);
    let mut base = x;
    let mut exp = n;

    while exp > 0 {
        if exp % 2 == 1 {
            result = result * base;
        }
        base = base * base;
        exp /= 2;
    }
    result
}

pub fn verify_proofs(
    proofs: &Vec<ProofUnit>,
    a: &DMatrix<ZqInt>,
    t: DVector<ZqInt>,
    u: DVector<ZqInt>,
    mut x: Vec<DVector<ZqInt>>,
    ell:usize,
    r: usize,
    kappa: usize,
    n: usize,
    q: ZqMod,
    log_q: usize,
) -> bool{
    let zero = ZqInt::new(0,q);
    let one = ZqInt::new(1,q);
    let left = i_kron_a_dot_s(&a, &proofs[0].y, kappa, &zero, &one);
    assert_eq!(left,t);
    let left = i_kron_vec_dot_vec(&x[ell], &proofs[0].v, kappa * n, q);
    assert_eq!(left,u);
    if infinity_norm(&proofs[0].y) as usize > (r*kappa).pow(0u32) {
        panic!("y_{}' norm is too large",0);
    }
    // println!("verify success for tree depth: {}",0);
    let mut c = generate_fiat_shamir_challenge_matrix(&t, &u, &x, &proofs[0].y, &proofs[0].v, r, kappa, q);
    for i in 1..=ell {
        let left = i_kron_a_dot_s(&a, &proofs[i].y, kappa, &zero, &one);
        let tj_plus_1 = c_kron_g_dot_y(&c, &proofs[i-1].y, n, q, log_q);
        assert_eq!(left,tj_plus_1);
        let left = i_kron_vec_dot_vec(&x[ell-i], &proofs[i].v, kappa * n, q);
        let uj_plus_1 = c_kron_i_dot_v(&c, &proofs[i-1].v, n, q);
        assert_eq!(left,uj_plus_1);
        if infinity_norm(&proofs[i].y) as usize > (r*kappa).pow(i as u32) {
            panic!("y_{}' norm is too large",i);
        }

        x.truncate(x.len() - 1);
        c = generate_fiat_shamir_challenge_matrix(&tj_plus_1, &uj_plus_1, &x, &proofs[i].y, &proofs[i].v, r, kappa, q);
        // println!("verify success for tree depth: {}",i);
    }
    true
}


pub fn dvector_zeros(m: usize, modulus: ZqMod) -> DVector<ZqInt> {
    let zeros = vec![ZqInt::new(0, modulus); m];
    DVector::from_vec(zeros)
}

pub fn infinity_norm(vec: &DVector<ZqInt>) -> ZqMod {
    vec.iter()
        .map(|v| {
            let res = v.residue();
            if res <= v.modulus() / 2 {
                res
            } else {
                v.modulus() - res
            }
        })
        .max()
        .unwrap_or(0)
}


/// calculate (I:κ ⊗ A:n × m)·s:κm
pub fn i_kron_a_dot_s(
    a: &DMatrix<ZqInt>,
    s: &DVector<ZqInt>,
    kappa: usize,
    zero: &ZqInt, // 保留 zero 用于初始化
    _one: &ZqInt, // one 未使用，但保留参数兼容性
) -> DVector<ZqInt> {
    let n = a.nrows();
    let m = a.ncols();
    assert_eq!(
        s.len(),
        kappa * m,
        "s length must be κ * m, expected {}, got {}",
        kappa * m,
        s.len()
    );

    // 并行计算 t 的每个 κ 块
    let t_chunks: Vec<_> = (0..kappa).into_par_iter().flat_map(|i| {
        let mut chunk = Vec::with_capacity(n);
        for p in 0..n {
            let mut sum = *zero;
            for q in 0..m {
                sum = sum + a[(p, q)] * s[i * m + q]; // 完整乘法
            }
            chunk.push(sum);
        }
        chunk
    }).collect();

    let mut t = Vec::with_capacity(kappa * n);
    t.extend(t_chunks);
    DVector::from_vec(t)
}
/// calculate (I ⊗ vec)·vec
pub fn i_kron_vec_dot_vec(a: &DVector<ZqInt>, s: &DVector<ZqInt>, k: usize, q: ZqMod) -> DVector<ZqInt> {
    let n = a.len();

    assert_eq!(
        s.len(),
        k * n,
        "Vector s length must be k * n"
    );

    let mut t = dvector_zeros(k, q);

    // 并行计算每个 t[i]
    t.as_mut_slice()
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, sum)| {
            *sum = (0..n).fold(ZqInt::new(0, q), |acc, j| {
                acc + a[j] * s[i * n + j]
            });
        });
    t
}

pub fn c_kron_g_dot_y(c_t: &DMatrix<ZqInt>, y: &DVector<ZqInt>, m: usize, q: ZqMod, log_q: usize) -> DVector<ZqInt> {
    let kappa = c_t.nrows();      // C 的行数
    let kappa_r = c_t.ncols();    // C 的列数

    // 验证输入向量的长度是否正确
    assert_eq!(
        y.len(),
        kappa_r * m * log_q,
        "Vector length must be kappa_r * m * ceil(log q)"
    );

    // 将 y 分成 kappa_r 个大块
    let y_blocks: Vec<DVector<ZqInt>> = (0..kappa_r)
        .map(|j| {
            let start = j * m * log_q;
            let end = start + m * log_q;
            let block_elements: Vec<ZqInt> = (start..end)
                .map(|idx| y[idx].clone())
                .collect();
            DVector::from_vec(block_elements)
        })
        .collect();

    // 并行计算 intermediate_blocks，利用左移替代 g^T
    let intermediate_blocks: Vec<DVector<ZqInt>> = y_blocks
        .par_iter()
        .map(|block| {
            let mut result = dvector_zeros(m, q);
            for i in 0..m {
                let sub_block_start = i * log_q;
                for j in 0..log_q {
                    let value = block[sub_block_start + j].residue(); // 提取数值
                    let shifted = value << j;         // 左移并模运算
                    let shifted_zq = ZqInt::new(shifted, q);
                    result[i] = result[i] + shifted_zq;
                }
            }
            result
        })
        .collect();

    // 并行计算结果块
    let mut result_blocks: Vec<DVector<ZqInt>> = Vec::with_capacity(kappa);
    result_blocks.resize_with(kappa, || dvector_zeros(m, q));

    result_blocks.par_iter_mut().enumerate().for_each(|(i, block)| {
        for j in 0..kappa_r {
            if c_t[(i, j)].residue() == 1 {
                *block += &intermediate_blocks[j];
            }
        }
    });

    // 合并结果
    let mut result_vec = Vec::with_capacity(kappa * m);
    for block in result_blocks {
        result_vec.extend(block.iter().cloned());
    }

    DVector::from_vec(result_vec)
}

pub fn c_kron_i_dot_v(c_t: &DMatrix<ZqInt>, v: &DVector<ZqInt>, m: usize, q: ZqMod) -> DVector<ZqInt> {
    let kappa = c_t.nrows();      // C 的行数
    let kappa_r = c_t.ncols();    // C 的列数

    // 验证输入向量的长度是否正确
    assert_eq!(
        v.len(),
        kappa_r * m,
        "Vector length must be kappa_r * m"
    );

    // 将 v 分成 kappa_r 个长度为 m 的块，避免 clone
    let v_blocks: Vec<DVector<ZqInt>> = (0..kappa_r)
        .map(|j| {
            let start = j * m;
            let end = start + m;
            let block_elements: Vec<ZqInt> = (start..end)
                .map(|idx| v[idx].clone())
                .collect();
            DVector::from_vec(block_elements)
        })
        .collect();

    // 并行计算结果块
    let mut result_blocks: Vec<DVector<ZqInt>> = Vec::with_capacity(kappa);
    result_blocks.resize_with(kappa, || dvector_zeros(m, q));

    result_blocks.par_iter_mut().enumerate().for_each(|(i, block)| {
        for j in 0..kappa_r {
            if c_t[(i, j)].residue() == 1 {
                *block += &v_blocks[j];
            }
        }
    });

    // 合并结果
    let mut result_vec = Vec::with_capacity(kappa * m);
    for block in result_blocks {
        result_vec.extend(block.iter().cloned());
    }

    DVector::from_vec(result_vec)
}