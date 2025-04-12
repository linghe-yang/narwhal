use std::fs::File;
use std::io::Write;
use std::path::Path;
#[cfg(not(feature = "pq"))]
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
#[cfg(not(feature = "pq"))]
use curve25519_dalek::{RistrettoPoint, Scalar};
#[cfg(feature = "pq")]
use rand::Rng;
#[cfg(not(feature = "pq"))]
use rand::rngs::OsRng;
use model::file_io::Import;
use serde::{Deserialize, Serialize};
#[cfg(feature = "pq")]
pub type ZqMod = u128;
#[cfg(not(feature = "pq"))]
#[derive(Debug, Clone,Serialize,Deserialize)]
pub struct CommonReferenceString {
    pub g: Vec<RistrettoPoint>,
    pub h: RistrettoPoint,
}
#[cfg(feature = "pq")]
#[derive(Debug, Clone,Serialize,Deserialize)]
pub struct CommonReferenceString {
    pub a: Vec<Vec<ZqMod>>,
    pub q: ZqMod,
    pub n: usize,
    pub kappa: usize,
    pub r: usize,
    pub ell: usize
}

impl Import for CommonReferenceString {}

#[cfg(feature = "pq")]
pub fn generate_crs(n: usize, m: usize, q: ZqMod) {
    let mut rng = rand::thread_rng();
    let a = (0..n)
        .map(|_| {
            (0..m)
                .map(|_| rng.gen_range(0..q))
                .collect::<Vec<ZqMod>>()
        })
        .collect::<Vec<Vec<ZqMod>>>();
    let crs = CommonReferenceString { a, q, n, kappa: 0, r: 0, ell: 0 };
    write_crs_to_json(&crs).expect("Failed to write crs to json");
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