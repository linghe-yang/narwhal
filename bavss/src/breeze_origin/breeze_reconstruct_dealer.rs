use curve25519_dalek::scalar::Scalar;
use model::types_and_const::{Id, RandomNum};

pub struct BreezeReconResult{
    pub value: Scalar,
}

impl BreezeReconResult {
    pub fn new(output: Scalar ) -> Self {
        BreezeReconResult{
            value: output,
        }
    }
    
    pub fn interpolate(evaluate_ids: &Vec<Id>, shares: &Vec<Scalar>) -> Scalar {
        let evaluate_points = Self::generate_evaluation_points_n(evaluate_ids);
        Self::lagrange_interpolation_at_zero(&evaluate_points,shares)
    }

    pub fn secret_to_number(&self) -> RandomNum {
        let bytes = self.value.to_bytes();
        RandomNum::from_le_bytes(bytes[..16].try_into().unwrap())
    }

    fn lagrange_interpolation_at_zero(points: &Vec<Scalar>, values: &Vec<Scalar>) -> Scalar {
        let mut result = Scalar::ZERO;

        for i in 0..points.len() {
            let mut term = values[i];

            for j in 0..points.len() {
                if i != j {
                    term *= -points[j] * (points[i] - points[j]).invert();
                }
            }
            result += term;
        }

        result
    }
    pub fn generate_evaluation_points_n(ids: &Vec<Id>) -> Vec<Scalar> {
        let mut res: Vec<Scalar> = Vec::new();

        for id in ids {
            let base = Scalar::from(*id as u64);
            res.push(base);
        }
        res
    }
}