//! CoolProp's multiparameter Helmholtz EOS: the built-in family.

use super::{
    GaoBTau, GaussianBlock, GaussianTau, HelmholtzModel, IdealGas, NonAnalyticBlock, PowerBlock, TauExpBlock,
    power::Vars,
};
use crate::derivs::{Derivs, Order, Virials};
use crate::error::{Error, LoadError};
use crate::exec::{self, ExecPolicy, LaneKernel};
use crate::num::{Lanes, Real};

/// Residual block kinds: a closed set compiled to SoA at decode, `match`ed once per block, never per term.
/// Six separable kinds cover 99.8 % of default terms (map 02); NonAnalytic stays scalar.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidualBlock {
    /// Power and Exponential terms. Every kind gets its δ-factors in a cancellation-free form and its exact δ → 0
    /// series (E4).
    Power(PowerBlock),
    /// Lemmon2005 and DoubleExponential terms: an exponential on the τ-side too.
    TauExp(TauExpBlock),
    /// Gaussian bell-shaped terms.
    Gaussian(GaussianBlock<GaussianTau>),
    /// GaoB terms (Ammonia).
    GaoB(GaussianBlock<GaoBTau>),
    /// Non-analytic terms (Water, CarbonDioxide): not separable, evaluated on a jet of the bundle's order.
    NonAnalytic(NonAnalyticBlock),
}

/// What the residual is evaluated on: `f64`, and `Lanes<W>` across W states (PLAN.md M12.2). NonAnalytic blocks are not
/// separable and run on their own jets of `f64`, so lanes evaluate them lane by lane at their place in block order:
/// each lane then adds its terms in the scalar order.
pub(crate) trait ResidualReal: Real {
    /// Adds a NonAnalytic block's terms to `acc`.
    fn non_analytic<const ORD: usize>(block: &NonAnalyticBlock, vars: &Vars<Self>, acc: &mut Derivs<Self>);
}

impl ResidualReal for f64 {
    #[inline(always)]
    fn non_analytic<const ORD: usize>(block: &NonAnalyticBlock, vars: &Vars<f64>, acc: &mut Derivs) {
        block.accumulate::<ORD>(vars.tau, vars.delta_pow[1], acc);
    }
}

impl<const W: usize> ResidualReal for Lanes<W> {
    #[inline(always)]
    fn non_analytic<const ORD: usize>(block: &NonAnalyticBlock, vars: &Vars<Self>, acc: &mut Derivs<Self>) {
        for (k, (tau, delta)) in vars.tau.0.iter().zip(vars.delta_pow[1].0).enumerate() {
            if let Some(mut lane) = acc.lane(k) {
                block.accumulate::<ORD>(*tau, delta, &mut lane);
                acc.set_lane(k, &lane);
            }
        }
    }
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

    #[inline(always)]
    fn eval<R: ResidualReal, const ORD: usize>(&self, vars: &Vars<R>, order: Order) -> Derivs<R> {
        let mut acc = Derivs::zero(order);
        for block in self.blocks.iter() {
            match block {
                ResidualBlock::Power(b) => b.accumulate::<R, ORD>(vars, &mut acc),
                ResidualBlock::TauExp(b) => b.accumulate::<R, ORD>(vars, &mut acc),
                ResidualBlock::Gaussian(b) => b.accumulate::<R, ORD>(vars, &mut acc),
                ResidualBlock::GaoB(b) => b.accumulate::<R, ORD>(vars, &mut acc),
                ResidualBlock::NonAnalytic(b) => R::non_analytic::<ORD>(b, vars, &mut acc),
            }
        }
        acc
    }

    /// The residual at W states side by side: lane `k` is, bit for bit, [`HelmholtzModel::residual`] at
    /// `(t.0[k], rho.0[k])` (PLAN.md M12.2).
    #[inline(always)]
    pub(crate) fn residual_lanes<const W: usize>(&self, t: Lanes<W>, rho: Lanes<W>, order: Order) -> [Derivs; W] {
        let vars = Vars::new(Lanes::from_f64(self.t_r) / t, rho / Lanes::from_f64(self.rho_r));
        let acc = match order {
            Order::One => self.eval::<Lanes<W>, 1>(&vars, order),
            Order::Two => self.eval::<Lanes<W>, 2>(&vars, order),
            Order::Three => self.eval::<Lanes<W>, 3>(&vars, order),
            Order::Four => self.eval::<Lanes<W>, 4>(&vars, order),
        };
        acc.split()
    }
}

/// The residual at one order as a lane kernel (PLAN.md M12.3): (T, ρ) to its bundle.
struct Residual<'a> {
    eos: &'a MultiParameterEos,
    order: Order,
}

impl LaneKernel for Residual<'_> {
    type Out = Derivs;
    #[inline(always)]
    fn point(&self, t: f64, rho: f64) -> Derivs {
        self.eos.residual(t, rho, self.order)
    }
    #[inline(always)]
    fn lanes<const W: usize>(&self, t: Lanes<W>, rho: Lanes<W>) -> [Derivs; W] {
        self.eos.residual_lanes(t, rho, self.order)
    }
}

impl HelmholtzModel for MultiParameterEos {
    fn gas_constant(&self) -> f64 {
        self.r
    }

    /// The residual's lane kernel under `exec`: `ExecPolicy::Simd` evaluates states side by side (PLAN.md M12.3).
    fn residual_batch(
        &self,
        t: &[f64],
        rho: &[f64],
        order: Order,
        out: &mut [Derivs],
        exec: ExecPolicy,
    ) -> Result<(), Error> {
        exec::run(exec, &Residual { eos: self, order }, t, rho, out)
    }

    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
        let vars = Vars::new(self.t_r / t, rho / self.rho_r);
        match order {
            Order::One => self.eval::<f64, 1>(&vars, order),
            Order::Two => self.eval::<f64, 2>(&vars, order),
            Order::Three => self.eval::<f64, 3>(&vars, order),
            Order::Four => self.eval::<f64, 4>(&vars, order),
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
                ResidualBlock::GaoB(b) => b.zero_density_series(tau),
                ResidualBlock::NonAnalytic(b) => b.zero_density_series(tau),
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

    /// Every entry's bits, `None` above the bundle's order; NaN canonical, as Rust leaves NaN bit patterns unspecified
    /// (an operand order LLVM commutes picks the other NaN's sign; VERIFICATION.md §9.4).
    #[cfg(feature = "embedded")]
    fn bits(d: &Derivs) -> Vec<Option<u64>> {
        let canonical = |v: f64| if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() };
        (0..=4).flat_map(|n| (0..=n).map(move |i| (i, n - i))).map(|(i, j)| d.get(i, j).map(canonical)).collect()
    }

    /// A SplitMix64 draw in `[a, b)`.
    #[cfg(feature = "embedded")]
    fn uniform(state: &mut u64, a: f64, b: f64) -> f64 {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        a + (b - a) * ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }

    /// `points` in chunks of W through `residual_lanes`, each lane against the scalar residual at every order.
    #[cfg(feature = "embedded")]
    fn assert_lanes_match<const W: usize>(name: &str, eos: &MultiParameterEos, points: &[(f64, f64)]) {
        for chunk in points.as_chunks::<W>().0 {
            let (t, rho): (Lanes<W>, Lanes<W>) =
                (Lanes(core::array::from_fn(|k| chunk[k].0)), Lanes(core::array::from_fn(|k| chunk[k].1)));
            for order in [Order::One, Order::Two, Order::Three, Order::Four] {
                for (k, got) in eos.residual_lanes(t, rho, order).iter().enumerate() {
                    let want = eos.residual(t.0[k], rho.0[k], order);
                    assert_eq!(bits(got), bits(&want), "{name} W={W} {order:?} lane {k}: T {} ρ {}", t.0[k], rho.0[k]);
                }
            }
        }
    }

    /// PLAN.md M12.2 (VERIFICATION.md §9.2, class `Exact`): on `Lanes<2>`, `Lanes<4>` and `Lanes<8>`, every embedded
    /// fluid's residual is, lane by lane and bit for bit, its scalar residual at that lane's state, at every order.
    /// The states span the fluid's T range and ρ up to `rho_max`, with the δ → 0 and τ = δ = 1 neighbourhoods (Water's
    /// and CarbonDioxide's non-analytic terms are singular there) and NaN inputs.
    #[cfg(feature = "embedded")]
    #[test]
    fn lanes_are_scalar_bitwise() {
        let registry = crate::Registry::embedded().unwrap();
        let mut state = 12;
        let mut fluids = 0;
        for name in registry.names() {
            let record = registry.record(name).unwrap();
            let eos = record.eos.compile().unwrap();
            let (t_min, t_max) = (record.limits.t_min(), record.limits.t_max());
            let mut points: Vec<(f64, f64)> =
                (0..40).map(|_| (uniform(&mut state, t_min, t_max), uniform(&mut state, 0.0, eos.rho_max))).collect();
            let (t_r, rho_r) = (eos.t_r, eos.rho_r);
            points.extend([
                (t_r, rho_r),
                (t_r * (1.0 + 1e-9), rho_r * (1.0 - 1e-9)),
                (t_max, 0.0),
                (t_min, 1e-8 * rho_r),
                (f64::NAN, rho_r),
                (t_r, f64::NAN),
                (t_min, eos.rho_max),
                (t_max, eos.rho_max),
            ]);
            assert_lanes_match::<2>(name, &eos, &points);
            assert_lanes_match::<4>(name, &eos, &points);
            assert_lanes_match::<8>(name, &eos, &points);
            fluids += 1;
        }
        assert!(fluids >= 130, "{fluids} fluids");
    }

    /// PLAN.md M12.4 (VERIFICATION.md §9.2, class `Exact`): `policy_equivalence` at every dispatch level of this CPU,
    /// forced down from `Level::new()`: the residual's lane kernel through `run_simd` is, for every embedded fluid at
    /// every order, the scalar residual at each point, bit for bit (NaN canonical). Batches of 0 to 17 points take
    /// every remainder of 2, 4 and 8 lanes; the whole set adds τ = δ = 1, ρ = 0, `rho_max` and NaN inputs.
    #[cfg(all(feature = "embedded", feature = "simd"))]
    #[test]
    fn policy_equivalence_at_every_level() {
        let registry = crate::Registry::embedded().unwrap();
        let (mut state, mut fluids) = (14, 0);
        let levels = crate::exec::tests::levels();
        for name in registry.names() {
            let record = registry.record(name).unwrap();
            let eos = record.eos.compile().unwrap();
            let (t_min, t_max) = (record.limits.t_min(), record.limits.t_max());
            let (mut t, mut rho): (Vec<f64>, Vec<f64>) =
                (0..20).map(|_| (uniform(&mut state, t_min, t_max), uniform(&mut state, 0.0, eos.rho_max))).unzip();
            t.extend([eos.t_r, t_max, t_min, f64::NAN, t_max]);
            rho.extend([eos.rho_r, 0.0, eos.rho_max, 1.0, f64::NAN]);
            for order in [Order::One, Order::Two, Order::Three, Order::Four] {
                for level in &levels {
                    assert_level_matches(name, &eos, *level, (&t, &rho), order);
                }
            }
            fluids += 1;
        }
        assert!(fluids >= 130 && !levels.is_empty(), "{fluids} fluids, levels {levels:?}");
    }

    /// One fluid, level and order of [`policy_equivalence_at_every_level`]: every prefix of 0 to 17 points and the
    /// whole set through `run_simd`, each point bit for bit the scalar residual.
    #[cfg(all(feature = "embedded", feature = "simd"))]
    fn assert_level_matches(
        name: &str,
        eos: &MultiParameterEos,
        level: fearless_simd::Level,
        (t, rho): (&[f64], &[f64]),
        order: Order,
    ) {
        let kernel = Residual { eos, order };
        for n in (0..=17).chain([t.len()]) {
            let mut out = vec![Derivs::zero(order); n];
            exec::run_simd(level, &kernel, &t[..n], &rho[..n], &mut out);
            for (i, got) in out.iter().enumerate() {
                let want = eos.residual(t[i], rho[i], order);
                assert_eq!(bits(got), bits(&want), "{name} {order:?} {level:?} n = {n} point {i}");
            }
        }
    }

    /// PLAN.md M12.2: every `Real` operation on `Lanes<4>` is, lane by lane, the `f64` operation, bit for bit, so a
    /// formula written over `Real` cannot tell the two apart. Two lanes are outside some functions' domains (NaN).
    #[test]
    fn lane_operations_are_scalar_operations() {
        let (x, y) = (Lanes([0.3, -1.7, 2.5e-9, 41.0]), Lanes([1.1, 0.75, -3.0, 0.0]));
        type Unary = (fn(Lanes<4>) -> Lanes<4>, fn(f64) -> f64);
        type Binary = (fn(Lanes<4>, Lanes<4>) -> Lanes<4>, fn(f64, f64) -> f64);
        let unary: [Unary; 12] = [
            (|a| a.exp(), |a: f64| Real::exp(a)),
            (|a| a.expm1(), |a: f64| Real::expm1(a)),
            (|a| a.ln(), |a: f64| Real::ln(a)),
            (|a| a.ln_1p(), |a: f64| Real::ln_1p(a)),
            (|a| a.powi(3), |a: f64| Real::powi(a, 3)),
            (|a| a.powi(-2), |a: f64| Real::powi(a, -2)),
            (|a| a.powf(0.37), |a: f64| Real::powf(a, 0.37)),
            (|a| a.sqrt(), |a: f64| Real::sqrt(a)),
            (|a| a.sinh(), |a: f64| Real::sinh(a)),
            (|a| a.cosh(), |a: f64| Real::cosh(a)),
            (|a| a.atan(), |a: f64| Real::atan(a)),
            (|a| -a.abs(), |a: f64| -Real::abs(a)),
        ];
        for (n, (lanes, scalar)) in unary.iter().enumerate() {
            for (k, got) in lanes(x).0.iter().enumerate() {
                let want = scalar(x.0[k]);
                assert!(got.to_bits() == want.to_bits() || got.is_nan() && want.is_nan(), "unary {n}, lane {k}");
            }
        }
        let binary: [Binary; 7] = [
            (|a, b| a + b, |a, b| a + b),
            (|a, b| a - b, |a, b| a - b),
            (|a, b| a * b, |a, b| a * b),
            (|a, b| a / b, |a, b| a / b),
            (|a, b| a + b.0[0], |a, b| a + b),
            (|a, b| a * b.0[0], |a, b| a * b),
            (|a, _| a + Lanes::from_f64(-2.5), |a, _| a + -2.5),
        ];
        for (n, (lanes, scalar)) in binary.iter().enumerate() {
            let scalar_y = |k: usize| if n == 4 || n == 5 { y.0[0] } else { y.0[k] };
            for (k, got) in lanes(x, y).0.iter().enumerate() {
                assert_eq!(got.to_bits(), scalar(x.0[k], scalar_y(k)).to_bits(), "binary {n}, lane {k}");
            }
        }
    }

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
