use std::fs::File;
use std::io::Write;
use std::path::Path;
#[cfg(not(feature = "pq"))]
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
#[cfg(not(feature = "pq"))]
use curve25519_dalek::{RistrettoPoint, Scalar};
use num_bigint::{BigUint, RandBigInt, ToBigUint};
use num_prime::nt_funcs::is_prime;
use num_traits::{One, ToPrimitive, Zero};
#[cfg(feature = "pq")]
use rand::Rng;
#[cfg(not(feature = "pq"))]
use rand::rngs::OsRng;
use model::file_io::Import;
use serde::{Deserialize, Serialize};
use model::breeze_universal::CommonReferenceString;
use model::types_and_const::ZqMod;
// #[cfg(feature = "pq")]
// pub type ZqMod = u128;
// #[cfg(not(feature = "pq"))]
// #[derive(Debug, Clone,Serialize,Deserialize)]
// pub struct CommonReferenceString {
//     pub g: Vec<RistrettoPoint>,
//     pub h: RistrettoPoint,
// }
// #[cfg(feature = "pq")]
// #[derive(Debug, Clone,Serialize,Deserialize)]
// pub struct CommonReferenceString {
//     pub a: Vec<Vec<ZqMod>>,
//     pub q: ZqMod,
//     pub n: usize,
//     pub kappa: usize,
//     pub r: usize,
//     pub ell: usize
// }

// impl Import for CommonReferenceString {}

#[cfg(feature = "pq")]
pub fn generate_crs(n: usize, log_q_approximate: u32, g: usize, kappa: usize, r: usize,ell: usize) {
    let q = match generate_large_prime(log_q_approximate-1).to_u128(){
        Some(q) => q,
        _ => {return;}
    } as ZqMod;
    let log_q = (q as f64).log2().ceil() as usize;
    let mut rng = rand::thread_rng();
    let m = r * n * log_q;
    let a = (0..n)
        .map(|_| {
            (0..m)
                .map(|_| rng.gen_range(0..q))
                .collect::<Vec<ZqMod>>()
        })
        .collect::<Vec<Vec<ZqMod>>>();
    let crs = CommonReferenceString { a, q,log_q,g, n, kappa, r, ell };
    write_crs_to_json(&crs).expect("Failed to write crs to json");
}

fn generate_large_prime(n: u32) -> BigUint {
    // 计算 2^n
    let two = 2.to_biguint().unwrap();
    let base = two.pow(n); // 2^n
    let upper_bound = two.pow(n + 1); // 2^(n+1) 作为上限

    let mut rng = rand::thread_rng();
    let mut candidate = base.clone();

    // 随机生成一个在 2^n 到 2^(n+1) 之间的数
    loop {
        // 在 base 和 upper_bound 之间随机选择
        let range = &upper_bound - &base;
        let offset = rng.gen_biguint_below(&range);
        candidate = base.clone() + offset;

        // 确保候选数是奇数（偶数不可能是素数，除了 2）
        if &candidate % 2u32 == Zero::zero() {
            candidate += BigUint::one();
        }

        // 使用 Miller-Rabin 测试素性
        if is_prime(&candidate, None).probably() {
            return candidate;
        }

        // 如果不是素数，继续尝试（这里简单递增，也可以随机重新生成）
        candidate += BigUint::from(2u32); // 每次加 2，保持奇数
        if candidate >= upper_bound {
            candidate = base.clone(); // 如果超出范围，重置到 base
        }
    }
}

#[cfg(not(feature = "pq"))]
pub fn generate_crs(t: usize) {
    let mut rng = OsRng;
    let scalar_h = Scalar::random(&mut rng);
    let point_h = RISTRETTO_BASEPOINT_POINT * scalar_h;

    let mut crs = CommonReferenceString {
        g: Vec::with_capacity(t + 1),
        h: point_h
    };

    for _ in 0..t+1 {
        let scalar_g = Scalar::random(&mut rng);
        let point_g = RISTRETTO_BASEPOINT_POINT * scalar_g;

        crs.g.push(point_g);
    }
    write_crs_to_json(&crs).expect("Failed to write crs to json");
}

fn write_crs_to_json(crs: &CommonReferenceString) -> std::io::Result<()> {
    #[cfg(feature = "benchmark")]
    let path = Path::new("./.crs.json");
    #[cfg(not(feature = "benchmark"))]
    let path = Path::new("benchmark/.crs.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;

    let json_string = serde_json::to_string_pretty(crs)?;
    file.write_all(json_string.as_bytes())?;
    Ok(())
}