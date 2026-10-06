//! CoolProp's multiparameter Helmholtz EOS: the built-in family.

use super::{GaussianBlock, HelmholtzModel, IdealGas, PowerBlock, TauExpBlock, power::Vars};
use crate::derivs::{Derivs, Order, Virials};
use crate::error::{Error, LoadError};

/// Residual block kinds: a closed set compiled to SoA at decode, `match`ed once per block, never per term.
/// Six separable kinds cover 99.8 % of default terms (map 02); NonAnalytic stays scalar.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidualBlock {
    /// Power and Exponential terms (M3.5 adds GaoB; M4 NonAnalytic). Every kind gets its δ-factors in a
    /// cancellation-free form and its exact δ → 0 series (E4).
    Power(PowerBlock),
    /// Lemmon2005 and DoubleExponential terms: an exponential on the τ-side too.
    TauExp(TauExpBlock),
    /// Gaussian bell-shaped terms.
    Gaussian(GaussianBlock),
}

/// The multiparameter EOS of one fluid: its own R, reducing state (private: never shared, map 06 C1),
/// residual blocks and ideal gas.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MultiParameterEos {
    r: f64,
    t_r: f64,
    rho_r: f64,
    rho_max: f64,
    blocks: Box<[ResidualBlock]>,
    ideal: IdealGas,
}

impl MultiParameterEos {
    /// Validates constants; blocks and ideal gas come validated.
    pub(crate) fn new(
        r: f64,
        t_r: f64,
        rho_r: f64,
        rho_max: f64,
        blocks: Vec<ResidualBlock>,
        ideal: IdealGas,
    ) -> Result<Self, Error> {
        if !(r > 0.0 && t_r > 0.0 && rho_r > 0.0 && rho_max > 0.0) {
            return Err(Error::Load(LoadError::Format("EOS constants must be > 0".into())));
        }
        Ok(Self { r, t_r, rho_r, rho_max, blocks: blocks.into_boxed_slice(), ideal })
    }

    fn eval<const ORD: usize>(&self, vars: &Vars<f64>, order: Order) -> Derivs {
        let mut acc = Derivs::zero(order);
        for block in self.blocks.iter() {
            match block {
                ResidualBlock::Power(b) => b.accumulate::<f64, ORD>(vars, &mut acc),
                ResidualBlock::TauExp(b) => b.accumulate::<f64, ORD>(vars, &mut acc),
                ResidualBlock::Gaussian(b) => b.accumulate::<f64, ORD>(vars, &mut acc),
            }
        }
        acc
    }
}

impl HelmholtzModel for MultiParameterEos {
    fn gas_constant(&self) -> f64 {
        self.r
    }

    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
        let vars = Vars::new(self.t_r / t, rho / self.rho_r);
        match order {
            Order::One => self.eval::<1>(&vars, order),
            Order::Two => self.eval::<2>(&vars, order),
            Order::Three => self.eval::<3>(&vars, order),
            Order::Four => self.eval::<4>(&vars, order),
        }
    }

    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs {
        self.ideal.eval(t, rho, order)
    }

    fn rho_max(&self, _t: f64) -> f64 {
        self.rho_max
    }

    /// `α^r = Σ a_k(τ) δ^k` near δ = 0, so `B = a₁/ρ_r` and `C = 2a₂/ρ_r²`; `T d/dT = −τ d/dτ`.
    fn zero_density(&self, t: f64) -> Option<Virials> {
        let tau = self.t_r / t;
        let mut a = [[0.0; 2]; 3];
        for block in self.blocks.iter() {
            let s = match block {
                ResidualBlock::Power(b) => b.zero_density_series(tau),
                ResidualBlock::TauExp(b) => b.zero_density_series(tau),
                ResidualBlock::Gaussian(b) => b.zero_density_series(tau),
            };
            for (acc, x) in a.iter_mut().flatten().zip(s.iter().flatten()) {
                *acc += x;
            }
        }
        let [_, [a1, ta1], [a2, ta2]] = a;
        let r2 = self.rho_r * self.rho_r;
        Some(Virials {
            b: a1 / self.rho_r,
            c: 2.0 * a2 / r2,
            db_dt: -ta1 / (t * self.rho_r),
            dc_dt: -2.0 * ta2 / (t * r2),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helmholtz::{IdealTerm, PowerTerm};
    use crate::num::math;

    fn toy() -> MultiParameterEos {
        let terms = [
            PowerTerm::new(0.5, 1.5, 2, 1, 1.0),
            PowerTerm::new(-0.7, 0.25, 1, 0, 0.0),
            PowerTerm::new(0.3, 3.0, 4, 2, 1.0),
        ];
        let ideal =
            IdealGas::new(300.0, 10_000.0, vec![IdealTerm::Lead { a1: 1.0, a2: 2.0 }, IdealTerm::LogTau { a: 2.5 }]);
        MultiParameterEos::new(
            8.314_462_618,
            300.0,
            10_000.0,
            30_000.0,
            vec![ResidualBlock::Power(PowerBlock::new(&terms).unwrap())],
            ideal.unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn ideal_delta_gives_mechanical_derivatives() {
        let eos = toy();
        let total = eos.residual(310.0, 900.0, Order::Four) + eos.ideal(310.0, 900.0, Order::Four);
        let mech = eos.residual(310.0, 900.0, Order::Four) + Derivs::IDEAL_DELTA;
        for j in 1..=4 {
            assert_eq!(total.get(0, j), mech.get(0, j));
        }
    }

    /// Toy terms by hand: −0.7 τ^0.25 δ gives a₁; 0.5 τ^1.5 δ² e^(−δ) gives a₂ (its e^(−δ) only adds δ³).
    #[test]
    fn virials_are_exact_taylor_coefficients() {
        let eos = toy();
        let t = 250.0;
        let tau: f64 = 300.0 / t;
        let (f1, f2) = (-0.7 * math::powf(tau, 0.25), 0.5 * math::powf(tau, 1.5));
        let v = eos.zero_density(t).unwrap();
        assert_eq!((v.b, v.c), (f1 / 10_000.0, 2.0 * f2 / 1e8));
        assert!(((v.db_dt / (-0.25 * f1 / (t * 10_000.0))) - 1.0).abs() < 1e-15);
        assert!(((v.dc_dt / (-2.0 * 1.5 * f2 / (t * 1e8))) - 1.0).abs() < 1e-15);
    }
}
