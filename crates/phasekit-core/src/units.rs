//! SI newtypes at the API edge (D5, dependencies R11). Construction validates, so NaN, Q = 5 and T < 0 are
//! unrepresentable (map 12 R9). Specific quantities carry their basis; the kernel works in molar SI `f64`.

use crate::error::Error;
use crate::input::Var;

/// Molar or mass basis of a specific quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Basis {
    /// Per mole (the kernel's internal basis, as in CoolProp, map 01 §1).
    Molar,
    /// Per kilogram.
    Mass,
}

pub(crate) fn finite(quantity: &'static str, value: f64) -> Result<f64, Error> {
    if value.is_finite() { Ok(value) } else { Err(Error::InvalidInput { quantity, value }) }
}

pub(crate) fn positive(quantity: &'static str, value: f64) -> Result<f64, Error> {
    if value.is_finite() && value > 0.0 { Ok(value) } else { Err(Error::InvalidInput { quantity, value }) }
}

pub(crate) fn unit_interval(quantity: &'static str, value: f64) -> Result<f64, Error> {
    if (0.0..=1.0).contains(&value) { Ok(value) } else { Err(Error::InvalidInput { quantity, value }) }
}

macro_rules! absolute {
    ($(#[$doc:meta])* $name:ident, $var:ident, $quantity:literal, $check:ident, $unit:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub struct $name(f64);

        impl $name {
            pub(crate) const VAR: Var = Var::$var;
            #[doc = concat!("Validates a value in ", $unit, ".")]
            pub fn new(value: f64) -> Result<Self, Error> {
                $check($quantity, value).map(Self)
            }
            #[doc = concat!("Value in ", $unit, ".")]
            pub const fn get(self) -> f64 {
                self.0
            }
            pub(crate) const fn raw(self) -> (f64, Basis) {
                (self.0, Basis::Molar)
            }
        }
    };
}

absolute!(
    /// Absolute temperature, K (finite, > 0).
    Temperature, T, "temperature", positive, "K"
);
absolute!(
    /// Absolute pressure, Pa (finite, > 0).
    Pressure, P, "pressure", positive, "Pa"
);
absolute!(
    /// Vapour quality, molar basis, in [0, 1] (no −1 sentinel: single-phase states report `None`).
    Quality, Q, "quality", unit_interval, "[0, 1]"
);

macro_rules! specific {
    ($(#[$doc:meta])* $name:ident, $var:ident, $quantity:literal, $check:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $name {
            value: f64,
            basis: Basis,
        }

        impl $name {
            pub(crate) const VAR: Var = Var::$var;
            /// Validates a molar value.
            pub fn molar(value: f64) -> Result<Self, Error> {
                Self::with_basis(value, Basis::Molar)
            }
            /// Validates a mass-specific value.
            pub fn mass(value: f64) -> Result<Self, Error> {
                Self::with_basis(value, Basis::Mass)
            }
            /// Validates a value in the given basis.
            pub fn with_basis(value: f64, basis: Basis) -> Result<Self, Error> {
                $check($quantity, value).map(|value| Self { value, basis })
            }
            /// The value as given.
            pub const fn value(self) -> f64 {
                self.value
            }
            /// The basis it was given in.
            pub const fn basis(self) -> Basis {
                self.basis
            }
            /// The molar value, given the molar mass `m` in kg/mol.
            pub fn to_molar(self, m: f64) -> f64 {
                Self::VAR.to_molar(self.value, self.basis, m)
            }
            pub(crate) const fn raw(self) -> (f64, Basis) {
                (self.value, self.basis)
            }
        }
    };
}

specific!(
    /// Density, mol/m³ or kg/m³ (finite, > 0).
    Density, D, "density", positive
);
specific!(
    /// Enthalpy, J/mol or J/kg (finite; its zero is the handle's reference state).
    Enthalpy, H, "enthalpy", finite
);
specific!(
    /// Entropy, J/(mol K) or J/(kg K) (finite).
    Entropy, S, "entropy", finite
);
specific!(
    /// Internal energy, J/mol or J/kg (finite).
    InternalEnergy, U, "internal energy", finite
);
