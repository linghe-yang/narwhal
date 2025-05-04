
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
    use num_bigint::{BigUint, RandBigInt};
    use num_prime::nt_funcs::is_prime;
    use num_traits::{One, ToPrimitive, Zero};
    use rand::Rng;
    use crate::breeze_pq::zq_int::ZqInt;
    use crate::breeze_structs::PQCrs;

    #[test]
    fn test_share() {
        let n = 16;
        let r = 7;
        let kappa = 16;
        let g = 4;
        let beacon_per_epoch = n * kappa / g;
        
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
        let shares = Shares::new(batch_size, 1, ids, 1, &crs);
        let mut size_mb = 0.0;
        for share in shares.0.0.iter() {
            let proof = &share.0.eval_proof;
            size_mb = calculate_proof_size_kb(&proof);
        }
        println!("Total size of proof: {:.3} KB", size_mb);
        println!("proof size per beacon: {:.3} KB", size_mb / beacon_per_epoch as f64);
    }

    fn calculate_proof_size_kb(proof: &[(Vec<ZqMod>, Vec<ZqMod>)]) -> f64 {
        let proof_vec_metadata = size_of::<Vec<(Vec<ZqMod>, Vec<ZqMod>)>>();
        let total_heap_size: usize = proof.iter().fold(0, |acc, (vec1, vec2)| {
            let vec_metadata_size = 2 * size_of::<Vec<ZqMod>>();
            let data_size = (vec1.len() + vec2.len()) * size_of::<ZqMod>();
            acc + vec_metadata_size + data_size
        });
        let total_size_bytes = proof_vec_metadata + total_heap_size;
        total_size_bytes as f64 / 1_024.0
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
        let mut rng = rand::thread_rng();

        // 计算 2^(n-1) 和 2^n，作为素数的大致范围
        let lower_bound = BigUint::one() << (n - 1); // 2^(n-1)
        let upper_bound = (BigUint::one() << n) + (BigUint::one() << (n / 2)); // 2^n + 2^(n/2)

        loop {
            // 在 [2^(n-1), 2^n + 2^(n/2)] 范围内随机生成一个数
            let range = &upper_bound - &lower_bound;
            let random_offset: BigUint = rng.gen_biguint_range(&BigUint::zero(), &range);
            let candidate = &lower_bound + random_offset;

            // 检查是否为偶数（最低位为 0 表示偶数）
            let is_even = (&candidate & BigUint::one()).is_zero();
            // 确保候选数是奇数（素数一定是奇数，除了 2）
            let candidate = if is_even {
                candidate + BigUint::one()
            } else {
                candidate
            };

            // 检查是否为素数
            if is_prime(&candidate, None).probably() {
                return candidate;
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
