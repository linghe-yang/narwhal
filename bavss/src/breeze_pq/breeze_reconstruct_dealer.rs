use sha2::{Digest as ShaDigest, Sha256};
use model::types_and_const::{Id, RandomNum, ZqMod};
use crate::Secret;

pub struct BreezeReconResult{
    pub value: Vec<Secret>,
}

impl BreezeReconResult {
    pub fn new(output: Vec<Secret> ) -> Self {
        BreezeReconResult{
            value: output,
        }
    }
    pub fn interpolate(evaluate_ids: &Vec<Id>, shares: &Vec<Vec<Secret>>, q: ZqMod, cumulated: &mut Vec<ZqMod>) {
        let evaluate_points = generate_evaluation_points_n(evaluate_ids, q);
        let shares_t = transpose(shares);
        for (idx,share) in shares_t.iter().enumerate() {
            let res = Self::lagrange_interpolation_at_zero(&evaluate_points, share, q);
            cumulated[idx] += res;
        }
    }
    
    
    
    pub fn secret_to_number(&self) -> RandomNum {
        let hash = vec_to_sha256(&self.value);
        let res = hash_to_u128(&hash);
        res
    }
    fn lagrange_interpolation_at_zero(points: &Vec<ZqMod>, values: &Vec<ZqMod>, q: ZqMod) -> ZqMod {
        let n = points.len();
        let mut result = 0;

        for i in 0..n {
            let xi = points[i];
            let yi = values[i];

            let mut term = yi;
            for j in 0..n {
                if i != j {
                    let xj = points[j];
                    let numerator = (q - xj) % q;
                    let denominator = (xi + q - xj) % q;
                    let denominator_inv = mod_inverse(denominator, q);
                    term = (term * numerator) % q;
                    term = (term * denominator_inv) % q;
                }
            }
            result = (result + term) % q;
        }

        result
    }

}
fn generate_evaluation_points_n(ids: &Vec<Id>, q: ZqMod) -> Vec<ZqMod> {
    let mut res: Vec<ZqMod> = Vec::new();

    for id in ids {
        let base = (*id as ZqMod) % q;
        res.push(base);
    }
    res
}
fn mod_inverse(a: ZqMod, q: ZqMod) -> ZqMod {
    let (g, x, _) = extended_gcd(a as i128, q as i128);
    if g != 1 {
        panic!("Modular inverse does not exist");
    }
    ((x % (q as i128) + q as i128) % (q as i128)) as ZqMod
}

fn extended_gcd(a: i128, b: i128) -> (i128, i128, i128) {
    if a == 0 {
        (b, 0, 1)
    } else {
        let (g, x, y) = extended_gcd(b % a, a);
        (g, y - (b / a) * x, x)
    }
}

fn transpose(shares: &Vec<Vec<Secret>>) -> Vec<Vec<Secret>> {
    if shares.is_empty() || shares[0].is_empty() {
        return Vec::new();
    }
    let rows = shares.len();
    let cols = shares[0].len();

    assert!(shares.iter().all(|row| row.len() == cols), "All rows must have the same length");

    let mut transposed = vec![vec![Secret::default(); rows]; cols];
    for i in 0..rows {
        for j in 0..cols {
            transposed[j][i] = shares[i][j].clone();
        }
    }
    transposed
}

fn vec_to_sha256(secrets: &Vec<Secret>) -> [u8; 32] {

    let concatenated: String = secrets.into_iter()
        .map(|s| s.to_string())
        .collect();


    let mut hasher = Sha256::new();
    hasher.update(concatenated);
    let result = hasher.finalize();

    result.into()
}

fn hash_to_u128(hash: &[u8; 32]) -> u128 {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    u128::from_be_bytes(bytes)
}