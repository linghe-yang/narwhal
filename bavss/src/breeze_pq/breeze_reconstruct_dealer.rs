use log::info;
use model::types_and_const::{Id, RandomNum, ZqMod};
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
    pub fn interpolate(evaluate_ids: &Vec<Id>, shares: &Vec<Secret>, q: ZqMod) -> Secret {
        
        let evaluate_points = generate_evaluation_points_n(evaluate_ids, q);
        Self::lagrange_interpolation_at_zero(&evaluate_points,shares, q)
    }
    
    pub fn secret_to_number(&self) -> RandomNum {
        self.value as RandomNum
    }

    // 根据 t+1 个点计算 f(0)
    fn lagrange_interpolation_at_zero(points: &Vec<ZqMod>, values: &Vec<ZqMod>, q: ZqMod) -> ZqMod {
        let n = points.len();
        let mut result = 0;

        for i in 0..n {
            let xi = points[i];
            let yi = values[i];

            // 计算拉格朗日基函数 L_i(0) = Π_{j ≠ i} (0 - x_j) / (x_i - x_j)
            let mut term = yi;
            for j in 0..n {
                if i != j {
                    let xj = points[j];
                    // 分子: 0 - x_j = -x_j
                    let numerator = (q - xj) % q; // 模 q 下的 -x_j
                    // 分母: x_i - x_j
                    let denominator = (xi + q - xj) % q; // 确保正数
                    // 计算模逆
                    let denominator_inv = mod_inverse(denominator, q);
                    // term *= numerator * denominator_inv (模 q)
                    term = (term * numerator) % q;
                    term = (term * denominator_inv) % q;
                }
            }
            // result += y_i * L_i(0)
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
    ((x % (q as i128) + q as i128) % (q as i128)) as u128
}

fn extended_gcd(a: i128, b: i128) -> (i128, i128, i128) {
    if a == 0 {
        (b, 0, 1)
    } else {
        let (g, x, y) = extended_gcd(b % a, a);
        (g, y - (b / a) * x, x)
    }
}