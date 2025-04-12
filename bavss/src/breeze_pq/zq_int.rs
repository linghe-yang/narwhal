use std::fmt;
use std::ops::{Add, AddAssign, Mul, MulAssign};
use num_modular::{ModularInteger, MontgomeryInt};
use serde::{Deserialize, Serialize};
use model::types_and_const::ZqMod;


#[derive(Clone, Copy, PartialEq)]
pub struct ZqInt(pub(crate) MontgomeryInt<ZqMod>);
impl ZqInt {
    pub fn new(value: ZqMod, modulus: ZqMod) -> Self {
        assert!(modulus > 0, "Modulus must be positive");
        ZqInt(MontgomeryInt::new(value, &modulus))
    }
    pub fn residue(&self) -> ZqMod {
        self.0.residue()
    }
    
    pub fn modulus(&self) -> ZqMod {
        self.0.modulus()
    }
}
impl Add for ZqInt {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        ZqInt(self.0 + other.0)
    }
}
impl Mul for ZqInt {
    type Output = Self;
    fn mul(self, other: Self) -> Self {
        ZqInt(self.0 * other.0)
    }
}
impl AddAssign for ZqInt {
    fn add_assign(&mut self, other: Self) {
        self.0 = self.0 + other.0;
    }
}
impl MulAssign for ZqInt {
    fn mul_assign(&mut self, other: Self) {
        self.0 = self.0 * other.0;
    }
}

impl fmt::Debug for ZqInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ZqInt {{ value: {}, modulus: {} }}", self.0.residue(), self.0.modulus())
    }
}
impl fmt::Display for ZqInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.residue())
    }
}