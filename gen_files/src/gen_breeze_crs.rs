use std::fs::File;
use std::io::Write;
use std::path::Path;
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::Scalar;
use rand::rngs::OsRng;
use model::breeze_structs::CommonReferenceString;

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