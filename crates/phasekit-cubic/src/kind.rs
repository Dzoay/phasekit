//! The three cubic kinds and their constants (map 06 §3.1; Bell and Deiters, IECR 60 (2021) 9983).

use core::f64::consts::SQRT_2;

use phasekit_core::DataSet;

/// A cubic equation of state, p = RT/(v − b) − a(T)/((v + Δ₁b)(v + Δ₂b)) (map 06 §3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// van der Waals: Δ₁ = Δ₂ = 0.
    Vdw,
    /// Soave-Redlich-Kwong (Soave 1972): Δ₁ = 1, Δ₂ = 0.
    Srk,
    /// Peng-Robinson (1976): Δ₁ = 1 + √2, Δ₂ = 1 − √2.
    Pr,
}

/// The dimensionless constants of a and b: a = Ω_a R²T_c²/p_c · α(T) and b = Ω_b RT_c/p_c.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Omegas {
    /// Ω_a.
    pub a: f64,
    /// Ω_b.
    pub b: f64,
}

impl Kind {
    /// Δ₁ and Δ₂.
    pub const fn deltas(self) -> (f64, f64) {
        match self {
            Kind::Vdw => (0.0, 0.0),
            Kind::Srk => (1.0, 0.0),
            Kind::Pr => (1.0 + SQRT_2, 1.0 - SQRT_2),
        }
    }

    /// The critical compressibility factor p_c/(ρ_c R T_c) of the exact constants: 3/8, 1/3, and for PR
    /// 0.307401308698704 (map 06 §8). With η_c = b/v_c the root of the critical conditions in (0, 1) (1/3,
    /// 2^(1/3) − 1, and for PR the root of 3η³ + 3η² + 3η − 1), Ω_b = η_c Z_c.
    pub const fn z_c(self) -> f64 {
        match self {
            Kind::Vdw => 0.375,
            Kind::Srk => 1.0 / 3.0,
            Kind::Pr => 0.307_401_308_698_703_86,
        }
    }

    /// Soave's m(ω), the slope of √α in 1 − √θ: SRK's 0.480 + 1.574ω − 0.176ω² (Soave 1972) and PR's
    /// 0.37464 + 1.54226ω − 0.26992ω² (Peng and Robinson 1976), as v8.0.0 writes them (map 06 §3.1); van der Waals has
    /// none.
    pub fn soave_m(self, omega: f64) -> Option<f64> {
        match self {
            Kind::Vdw => None,
            Kind::Srk => Some(0.480 + 1.574 * omega - 0.176 * omega * omega),
            Kind::Pr => Some(0.37464 + 1.54226 * omega - 0.26992 * omega * omega),
        }
    }

    /// Ω_a and Ω_b. `Corrected`: the values the critical conditions give exactly, 27/64 and 1/8,
    /// 1/(9(2^(1/3) − 1)) and (2^(1/3) − 1)/3, and PR's. `Parity`: CoolProp 8.0.0's literals, whose comments state the
    /// same exact forms but whose digits differ: SRK's Ω_a by −4.36e-10 and Ω_b by +3.64e-10, PR's Ω_b by −5.0e-14
    /// (DIV-0047). van der Waals is not in v8.0.0 and has the exact values in both.
    pub const fn omegas(self, data: DataSet) -> Omegas {
        match (self, data) {
            (Kind::Vdw, _) => Omegas { a: 0.421_875, b: 0.125 },
            (Kind::Srk, DataSet::Corrected) => Omegas { a: 0.427_480_233_540_341_4, b: 0.086_640_349_964_957_72 },
            (Kind::Srk, DataSet::Parity) => Omegas { a: 0.427_480_233_354_034_1, b: 0.086_640_349_996_495_77 },
            (Kind::Pr, DataSet::Corrected) => Omegas { a: 0.457_235_528_921_382_2, b: 0.077_796_073_903_888_46 },
            (Kind::Pr, DataSet::Parity) => Omegas { a: 0.457_235_528_921_382_2, b: 0.077_796_073_903_884_56 },
        }
    }
}
