//! Typed inputs (D5): the 19 physical pairs of map 01 §4b, ONE table that generates the pair enum, its
//! variables and the typed constructors (map 01 R2-R4), a single validation gate, and declared capabilities
//! (map 01 R21). A model never re-validates: it receives a [`NativeInput`].

use crate::error::Error;
use crate::fluid::Gauge;
use crate::units::{
    Basis, Density, Enthalpy, Entropy, InternalEnergy, Pressure, Quality, Temperature, finite, positive, unit_interval,
};

/// A state variable that can appear in an input pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Var {
    /// Temperature.
    T,
    /// Pressure.
    P,
    /// Vapour quality.
    Q,
    /// Density.
    D,
    /// Enthalpy.
    H,
    /// Entropy.
    S,
    /// Internal energy.
    U,
}

impl Var {
    /// Validates a raw value of this variable: the rule of its newtype, applied once at the edge.
    fn validate(self, v: f64) -> Result<f64, Error> {
        match self {
            Var::T => positive("temperature", v),
            Var::P => positive("pressure", v),
            Var::Q => unit_interval("quality", v),
            Var::D => positive("density", v),
            Var::H => finite("enthalpy", v),
            Var::S => finite("entropy", v),
            Var::U => finite("internal energy", v),
        }
    }

    /// The basis a value of this variable carries: `basis` for D, H, S, U; molar for the basis-free T, P, Q.
    const fn basis_of(self, basis: Basis) -> Basis {
        match self {
            Var::D | Var::H | Var::S | Var::U => basis,
            Var::T | Var::P | Var::Q => Basis::Molar,
        }
    }

    /// Converts a value given in `basis` to molar with the molar mass `m` (kg/mol): the one basis table.
    pub(crate) fn to_molar(self, v: f64, basis: Basis, m: f64) -> f64 {
        match (self, basis) {
            (Var::D, Basis::Mass) => v / m,
            (Var::H | Var::S | Var::U, Basis::Mass) => v * m,
            _ => v,
        }
    }
}

/// One validated input: a pair and two values in pair order, each with the basis it was given in.
/// Build it with a typed constructor (`Input::dt(d, t)`, generated from the pair table) or from raw values
/// with [`Input::new`], the single gate for batch, string, C and JS callers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input {
    pair: Pair,
    x: (f64, Basis),
    y: (f64, Basis),
}

impl Input {
    /// Validates raw values in pair order; `basis` applies to D, H, S and U.
    pub fn new(pair: Pair, x: f64, y: f64, basis: Basis) -> Result<Input, Error> {
        let (vx, vy) = pair.vars();
        Ok(Input { pair, x: (vx.validate(x)?, vx.basis_of(basis)), y: (vy.validate(y)?, vy.basis_of(basis)) })
    }

    /// The physical pair.
    pub const fn pair(&self) -> Pair {
        self.pair
    }

    /// The two values as given, in pair order.
    pub const fn values(&self) -> (f64, f64) {
        (self.x.0, self.y.0)
    }

    /// Molar values in the model's native gauge: what a model receives.
    pub(crate) fn to_native(self, m: f64, gauge: Gauge) -> NativeInput {
        let (vx, vy) = self.pair.vars();
        let x = gauge.to_native(vx, vx.to_molar(self.x.0, self.x.1, m));
        let y = gauge.to_native(vy, vy.to_molar(self.y.0, self.y.1, m));
        NativeInput { pair: self.pair, x, y }
    }
}

/// An input as a [`crate::ThermoModel`] receives it: validated, molar SI, in pair order, in the model's
/// native gauge. Only [`crate::Fluid`] builds one, exactly once per call, so models never re-validate,
/// convert bases or see a reference state (map 15).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeInput {
    pair: Pair,
    x: f64,
    y: f64,
}

impl NativeInput {
    /// The physical pair.
    pub const fn pair(&self) -> Pair {
        self.pair
    }
    /// `(x, y)` in pair order, molar SI.
    pub const fn values(&self) -> (f64, f64) {
        (self.x, self.y)
    }
}

/// The pair table: one line per pair generates the `Pair` variant, `Pair::ALL`, `Pair::vars` (from the
/// quantity types) and the typed constructor of `Input`. A new pair is one new line (DRY, E11).
macro_rules! pairs {
    ($($pair:ident $ctor:ident($x:ident: $xt:ident, $y:ident: $yt:ident);)*) => {
        /// The 19 physical input pairs (5 with quality, 14 without), independent of basis.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        #[allow(missing_docs)] // the variant names are the documentation: (x, y) in name order
        pub enum Pair {
            $($pair),*
        }

        impl Pair {
            /// Every pair, in declaration order.
            pub const ALL: &'static [Pair] = &[$(Pair::$pair),*];

            /// The variables of `x` and `y`, in name order.
            pub const fn vars(self) -> (Var, Var) {
                match self {
                    $(Pair::$pair => ($xt::VAR, $yt::VAR)),*
                }
            }
        }

        impl Input {
            $(
                #[doc = concat!("A `", stringify!($pair), "` input from validated quantities.")]
                pub const fn $ctor($x: $xt, $y: $yt) -> Input {
                    Input { pair: Pair::$pair, x: $x.raw(), y: $y.raw() }
                }
            )*
        }
    };
}

pairs! {
    QT qt(q: Quality, t: Temperature);
    PQ pq(p: Pressure, q: Quality);
    QS qs(q: Quality, s: Entropy);
    HQ hq(h: Enthalpy, q: Quality);
    DQ dq(d: Density, q: Quality);
    PT pt(p: Pressure, t: Temperature);
    DT dt(d: Density, t: Temperature);
    HT ht(h: Enthalpy, t: Temperature);
    ST st(s: Entropy, t: Temperature);
    TU tu(t: Temperature, u: InternalEnergy);
    DP dp(d: Density, p: Pressure);
    HP hp(h: Enthalpy, p: Pressure);
    PS ps(p: Pressure, s: Entropy);
    PU pu(p: Pressure, u: InternalEnergy);
    HS hs(h: Enthalpy, s: Entropy);
    SU su(s: Entropy, u: InternalEnergy);
    DH dh(d: Density, h: Enthalpy);
    DS ds(d: Density, s: Entropy);
    DU du(d: Density, u: InternalEnergy);
}

impl Pair {
    /// The pair for two variables in either order; `swapped` is true when `(a, b)` is `(y, x)`.
    pub fn from_vars(a: Var, b: Var) -> Option<(Pair, bool)> {
        Pair::ALL.iter().find_map(|&p| match p.vars() {
            (x, y) if (x, y) == (a, b) => Some((p, false)),
            (x, y) if (x, y) == (b, a) => Some((p, true)),
            _ => None,
        })
    }
}

/// Declared input pairs of a model (a private, 64-wide bitset over [`Pair`]). The handle refuses
/// undeclared pairs with `Error::Unsupported`; the capability-matrix test checks that every declared pair
/// round-trips.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Capabilities(u64);

impl Capabilities {
    /// No pairs.
    pub const fn none() -> Self {
        Self(0)
    }
    /// Adds a pair.
    pub const fn with(self, pair: Pair) -> Self {
        Self(self.0 | 1 << pair as u32)
    }
    /// Pairs declared by both.
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
    /// Is `pair` declared?
    pub const fn contains(self, pair: Pair) -> bool {
        self.0 & (1 << pair as u32) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E12: the raw gate never panics and lets through only representable values.
    #[test]
    fn raw_gate_refuses_every_invalid_value_without_panicking() {
        let specials = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, 0.5, 1.0, 2.0, 1e300, -1e300];
        for &pair in Pair::ALL {
            for x in specials {
                for y in specials {
                    for basis in [Basis::Molar, Basis::Mass] {
                        let Ok(input) = Input::new(pair, x, y, basis) else { continue };
                        let (vx, vy) = pair.vars();
                        for (var, v) in [(vx, input.values().0), (vy, input.values().1)] {
                            assert!(v.is_finite());
                            match var {
                                Var::T | Var::P | Var::D => assert!(v > 0.0),
                                Var::Q => assert!((0.0..=1.0).contains(&v)),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(Pair::ALL.len(), 19);
    }

    #[test]
    fn typed_and_raw_constructors_agree() {
        let typed = Input::dt(Density::mass(996.5).unwrap(), Temperature::new(300.0).unwrap());
        assert_eq!(Input::new(Pair::DT, 996.5, 300.0, Basis::Mass).unwrap(), typed);
        assert_eq!(Pair::from_vars(Var::T, Var::D), Some((Pair::DT, true)));
    }
}
