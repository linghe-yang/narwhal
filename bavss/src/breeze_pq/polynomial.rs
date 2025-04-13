use nalgebra::DVector;
use crate::breeze_pq::zq_int::ZqInt;
use rand::Rng;
use model::types_and_const::ZqMod;

#[derive(Debug, Clone)]
pub struct Polynomial {
    coefficients: DVector<ZqInt>,
    modulus: ZqMod,
}

impl Polynomial {
    pub fn new(degree: usize, modulus: ZqMod) -> Self {
        assert!(modulus > 0, "Modulus must be positive");

        let mut rng = rand::thread_rng();
        let mut coeffs = Vec::with_capacity(degree + 1);
        

        for _ in 0..=degree {
            let coeff = rng.gen_range(0..modulus);
            coeffs.push(ZqInt::new(coeff, modulus));
        }

        Polynomial {
            coefficients: DVector::from_vec(coeffs),
            modulus,
        }
    }
    
    pub fn degree(&self) -> usize {
        self.coefficients.len() - 1
    }
    
    pub fn get_coeff(&self, index: usize) -> ZqInt {
        if index >= self.coefficients.len() {
            ZqInt::new(0, self.modulus)
        } else {
            self.coefficients[index]
        }
    }
    
    // pub fn coefficients(&self) -> &DVector<ZqInt> {
    //     &self.coefficients
    // }
    
    // pub fn modulus(&self) -> ZqMod {
    //     self.modulus
    // }

    // pub fn evaluate(&self, x: ZqInt) -> ZqInt {
    //     assert_eq!(x.modulus(), self.modulus, "Modulus mismatch: {} != {}", x.modulus(), self.modulus);
    // 
    //     let mut result = ZqInt::new(0, self.modulus);
    //     for &coeff in self.coefficients.iter().rev() {
    //         result = result * x + coeff;
    //     }
    //     result
    // }
}

// 实现多项式加法
impl std::ops::Add for Polynomial {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        assert_eq!(self.modulus, other.modulus, "Modulus must match");

        let max_degree = self.degree().max(other.degree());
        let mut coeffs = Vec::with_capacity(max_degree + 1);

        for i in 0..=max_degree {
            let sum = self.get_coeff(i) + other.get_coeff(i);
            coeffs.push(sum);
        }

        Polynomial {
            coefficients: DVector::from_vec(coeffs),
            modulus: self.modulus,
        }
    }
}

// 实现多项式乘法
impl std::ops::Mul for Polynomial {
    type Output = Self;
    fn mul(self, other: Self) -> Self {
        assert_eq!(self.modulus, other.modulus, "Modulus must match");

        let new_degree = self.degree() + other.degree();
        let mut coeffs = vec![ZqInt::new(0, self.modulus); new_degree + 1];

        for i in 0..=self.degree() {
            for j in 0..=other.degree() {
                let prod = self.get_coeff(i) * other.get_coeff(j);
                coeffs[i + j] = coeffs[i + j] + prod;
            }
        }

        Polynomial {
            coefficients: DVector::from_vec(coeffs),
            modulus: self.modulus,
        }
    }
}

impl std::fmt::Display for Polynomial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut terms = Vec::new();
        for (i, &coeff) in self.coefficients.iter().enumerate() {
            let coeff_val = coeff.residue();
            if coeff_val != 0 {
                let term = match i {
                    0 => format!("{}", coeff_val),
                    1 => format!("{}x", coeff_val),
                    _ => format!("{}x^{}", coeff_val, i),
                };
                terms.push(term);
            }
        }
        if terms.is_empty() {
            write!(f, "0")
        } else {
            write!(f, "{}", terms.join(" + "))
        }
    }
}