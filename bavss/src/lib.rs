
#[cfg(not(feature = "pq"))]
pub(crate) mod breeze_origin;
#[cfg(not(feature = "pq"))]
pub use breeze_origin::breeze::Breeze;
#[cfg(not(feature = "pq"))]
pub(crate) type Secret = curve25519_dalek::Scalar;

#[cfg(feature = "pq")]
pub(crate) mod breeze_pq;
#[cfg(feature = "pq")]
pub use breeze_pq::breeze::Breeze;
#[cfg(feature = "pq")]
pub(crate) type Secret = ZqMod;
mod merkletree;
#[cfg(feature = "pq")]
use model::types_and_const::ZqMod;



mod breeze_structs;
#[cfg(feature = "pq")]
#[cfg(test)]
mod test {
    use nalgebra::DMatrix;
    use crate::breeze_pq::breeze_share_dealer::Shares;
    use crypto::PublicKey;
    use model::types_and_const::{Id, ZqMod};
    use num_bigint::{BigUint, RandBigInt, ToBigUint};
    use num_prime::nt_funcs::is_prime;
    use num_traits::{One, ToPrimitive, Zero};
    use rand::Rng;
    use crate::breeze_pq::zq_int::ZqInt;
    use crate::breeze_structs::PQCrs;

    #[test]
    fn test_share() {
        let n = 8;
        let r = 7;
        let kappa = 8;
        
        let batch_size = (n* kappa) / 4;
        let q: ZqMod;
        if let Some(m) = generate_large_prime(32).to_u64() {
            q = m;
        } else {
            return;
        }
        let log_q = (q as f64).log2().ceil() as usize;
        let m = r * n * log_q;
        let crs = generate_crs_test(n, kappa, m, q, log_q, r);
        let ids = generate_ids(4);
        println!("预生成完成");
        let shares = Shares::new(batch_size, 1, ids, 1, &crs);
    }
    fn generate_crs_test(
        n: usize,
        kappa: usize,
        m: usize,
        q: ZqMod,
        log_q: usize,
        r: usize,
    ) -> PQCrs {
        let mut rng = rand::thread_rng();
        let a = (0..n)
            .map(|_| (0..m).map(|_| rng.gen_range(0..q)).collect::<Vec<ZqMod>>())
            .collect::<Vec<Vec<ZqMod>>>();
        let nrows = a.len();
        assert!(nrows > 0, "Matrix must have at least one row");
        let ncols = a[0].len();
        assert!(ncols > 0, "Matrix must have at least one column");
        assert!(a.iter().all(|row| row.len() == ncols), "All rows must have the same length");
        let flat_data: Vec<ZqInt> = a.into_iter()
            .flat_map(|row| {
                row.into_iter()
                    .map(|val| ZqInt::new(val, q))
            })
            .collect();
        
        PQCrs {
            a: DMatrix::from_vec(nrows, ncols, flat_data),
            q,
            log_q,
            g: 4,
            n,
            kappa,
            r,
            ell: 1,
        }
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

    fn generate_ids(n: usize) -> Vec<(PublicKey, Id)> {
        let mut ids = Vec::new();
        for id in 0..n {
            let pk = PublicKey::new_random_test();
            ids.push((pk, id));
        }
        ids
    }
}
