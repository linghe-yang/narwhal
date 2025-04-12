
use model::types_and_const::{Id, RandomNum};
use crate::Secret;

pub struct BreezeReconResult{
    pub value: Secret,
    // pub epoch: Epoch,
    // pub index: usize,
}

impl BreezeReconResult {
    pub fn new(output: Secret ) -> Self {
        BreezeReconResult{
            value: output,
            // epoch,
            // index
        }
    }
    #[cfg(not(feature = "pq"))]
    pub fn interpolate(evaluate_ids: &Vec<Id>, shares: &Vec<Secret>) -> Secret {
        let evaluate_points = Self::generate_evaluation_points_n(evaluate_ids);
        Self::lagrange_interpolation_at_zero(&evaluate_points,shares)
    }
    #[cfg(feature = "pq")]
    pub fn interpolate(evaluate_ids: &Vec<Id>, shares: &Vec<Secret>) -> Secret {
        // let evaluate_points = Self::generate_evaluation_points_n(evaluate_ids);
        // Self::lagrange_interpolation_at_zero(&evaluate_points,shares)
        0
    }

    // 计算拉格朗日基多项式 l_i(0) 的值
    // fn lagrange_basis_at_zero(x_points: &Vec<Scalar>, i: usize) -> Scalar {
    //     let mut numerator = Scalar::ONE;
    //     let mut denominator = Scalar::ONE;
    //     let xi = x_points[i];
    // 
    //     for (j, &xj) in x_points.iter().enumerate() {
    //         if j != i {
    //             numerator *= Scalar::ZERO - xj; // 分子：(0 - x_j)
    //             denominator *= xi - xj;         // 分母：(x_i - x_j)
    //         }
    //     }
    // 
    //     numerator * denominator.invert() // l_i(0) = numerator / denominator
    // }
    
    #[cfg(not(feature = "pq"))]
    pub fn secret_to_number(&self) -> RandomNum {
        let bytes = self.value.to_bytes(); // 获取底层 [u8; 32]
        u64::from_le_bytes(bytes[..8].try_into().unwrap()) // 取低8字节转为u64
    }
    #[cfg(feature = "pq")]
    pub fn secret_to_number(&self) -> RandomNum {
        0
    }

    // 根据 t+1 个点计算 f(0)
    #[cfg(not(feature = "pq"))]
    fn lagrange_interpolation_at_zero(points: &Vec<Scalar>, values: &Vec<Scalar>) -> Scalar {
        let mut result = Scalar::ZERO;

        for i in 0..points.len() {
            let mut term = values[i];

            // 计算拉格朗日基函数在x=0处的值
            for j in 0..points.len() {
                if i != j {
                    // L_i(0) = ∏(0-x_j)/(x_i-x_j) = ∏(-x_j)/(x_i-x_j)
                    term *= -points[j] * (points[i] - points[j]).invert();
                }
            }
            result += term;
        }

        result
    }
    #[cfg(not(feature = "pq"))]
    pub fn generate_evaluation_points_n(ids: &Vec<Id>) -> Vec<Scalar> {
        let mut res: Vec<Scalar> = Vec::new();

        for id in ids {
            let base = Scalar::from(*id as u64);
            res.push(base);
        }
        res
    }
}