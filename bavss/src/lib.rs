
pub mod breeze;
pub use breeze::breeze::Breeze;
pub use breeze::{generate_crs_file,load_crs};


#[cfg(test)]
mod tests {
    use crypto::PublicKey;
    use crate::breeze::{generate_crs, verify_merkle_proof, Shares};

    #[test]
    fn test_shares()  {
        let t = 1;
        let crs = generate_crs(t);
        let ids = vec![(PublicKey([0u8; 32]), 1),(PublicKey([0u8; 32]), 2),(PublicKey([0u8; 32]), 3),(PublicKey([0u8; 32]), 4)];

        let shares = Shares::new(5, 1, ids.clone(), t, &crs);

        for (share,(pk,id)) in shares.shares {

            let res = Shares::verify(&crs, id, t, share.clone());
            if !res {
                println!("fail to verify share for id:{:?}", id);
            }

            for (k,wit) in share.r_witness.iter().enumerate(){

                let commit = wit.poly_commit;
                let poly_commit_data = commit.compress().to_bytes().to_vec();
                match verify_merkle_proof(&poly_commit_data, wit.merkle_branch.clone(), share.c.clone(), share.r_witness.len()) {
                    Ok(res)=>{
                        if !res {
                            println!("fail to verify commit for witness index:{} of id: {}",k, id);
                            return
                        } else {
                            println!("success for {} in id: {}!",k,id);
                        }
                    }
                    Err(_)=>{
                        panic!("panic when verifying merkle witness result");
                    }
                }
            }
        }
    }
}