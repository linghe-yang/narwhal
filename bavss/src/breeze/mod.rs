pub mod breeze;
mod breeze_confirm;
mod breeze_message_handler;
// mod breeze_out;
mod breeze_reconstruct;
mod breeze_reply;
mod breeze_result;
mod breeze_share;
mod batch_eval;
mod breeze_share_dealer;
mod merkletree;
mod utils;
mod breeze_reconstruct_dealer;

// #[cfg(feature = "breeze_origin")]
pub use crs::{generate_crs_file,load_crs};
pub(crate) use breeze_share_dealer::Shares;
pub(crate) use crs::generate_crs;
pub(crate) use merkletree::verify_merkle_proof;

// #[cfg(feature = "breeze_origin")]
mod crs {
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::Path;
    use config::{Committee, Import};
    use anyhow::{Context};
    use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
    use curve25519_dalek::Scalar;
    use rand::rngs::OsRng;
    use model::breeze_structs::CommonReferenceString;

    pub fn load_crs() -> std::io::Result<CommonReferenceString> {
        #[cfg(feature = "benchmark")]
        let path = Path::new("./crs.json");
        #[cfg(not(feature = "benchmark"))]
        let path = Path::new("bavss/src/breeze/crs.json");
        let mut file = File::open(path)?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let crs: CommonReferenceString = serde_json::from_str(&contents)?;
        Ok(crs)
    }

    pub fn generate_crs(t: usize) -> CommonReferenceString {
        let mut rng = OsRng;
        let scalar_h = Scalar::random(&mut rng);
        let point_h = RISTRETTO_BASEPOINT_POINT * scalar_h;
        let mut crs = CommonReferenceString {
            g: Vec::with_capacity(t + 1),
            h: point_h
        };

        // 生成degree+1个随机点
        for _ in 0..t+1 {
            // 生成随机标量
            let scalar_g = Scalar::random(&mut rng);
            // 使用基点乘以随机标量来获得随机点
            let point_g = RISTRETTO_BASEPOINT_POINT * scalar_g;

            crs.g.push(point_g);
        }
        crs
    }

    fn write_crs_to_json(config: &CommonReferenceString) -> std::io::Result<()> {
        let path = Path::new("bavss/src/breeze/crs.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(path)?;

        let json_string = serde_json::to_string_pretty(config)?;
        file.write_all(json_string.as_bytes())?;
        Ok(())
    }
    pub fn generate_crs_file(committee: &Committee) {
        let t = committee.authorities_fault_tolerance();
        let crs = generate_crs(t);
        write_crs_to_json(&crs).expect("fail to write crs");
    }

}



