#[cfg(not(feature = "pq"))]
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
#[cfg(not(feature = "pq"))]
use curve25519_dalek::Scalar;
use model::breeze_universal::CommonReferenceString;
#[cfg(feature = "pq")]
use model::types_and_const::ZqMod;
#[cfg(feature = "pq")]
use num_bigint::{BigUint, RandBigInt};
#[cfg(feature = "pq")]
use num_prime::nt_funcs::is_prime;
#[cfg(feature = "pq")]
use num_traits::{One, ToPrimitive, Zero};
#[cfg(not(feature = "pq"))]
use rand::rngs::OsRng;
#[cfg(feature = "pq")]
use rand::Rng;
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[cfg(feature = "pq")]
pub fn generate_crs(n: usize, log_q_approximate: u32, g: usize, kappa: usize, r: usize,ell: usize) {
    let q = match generate_large_prime(log_q_approximate).to_u128(){
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

#[cfg(feature = "pq")]
fn generate_large_prime(n: u32) -> BigUint {
    let mut rng = rand::thread_rng();

    let lower_bound = BigUint::one() << (n - 1); // 2^(n-1)
    let upper_bound = (BigUint::one() << n) + (BigUint::one() << (n / 2)); // 2^n + 2^(n/2)

    loop {
        let range = &upper_bound - &lower_bound;
        let random_offset: BigUint = rng.gen_biguint_range(&BigUint::zero(), &range);
        let candidate = &lower_bound + random_offset;
        let is_even = (&candidate & BigUint::one()).is_zero();
        let candidate = if is_even {
            candidate + BigUint::one()
        } else {
            candidate
        };
        if is_prime(&candidate, None).probably() {
            return candidate;
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