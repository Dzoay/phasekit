//! The moist-air state at (T, p, ψ_w) (PLAN.md M18.3): RP-1485's virial mixture as v8.0.0's `HumidAirProp.cpp` writes
//! it (map 07 §3.3). The molar volume solves p = RT/v̄ (1 + B_m/v̄ + C_m/v̄²) by Newton; h̄, s̄ and ū add the virials'
//! departures to the ideal parts (`ideal`); c_p, c_v and w are the mixture's own derivatives, from the virials' first
//! two T-derivatives (user decision HA6), where v8.0.0 takes finite differences of h̄ and ū (ROT-269).

use phasekit_core::{DomainError, Error, Strategy, math};

use crate::constants::{M_AIR, M_WATER, R};
use crate::ideal::{self, R_AIR, Zeros};
use crate::quantities::WATER_MOLE_FRACTION;
use crate::virials::{Coefficient, Virials};
use crate::{HumidAir, humidity};

/// What a moist-air state's specific property is per.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Per {
    /// Per kilogram of dry air, as `HAPropsSI`'s H, S, U, V, C and CV are.
    KgDryAir,
    /// Per kilogram of humid air, as its Hha, Sha, Uha, Vha, Cha and CVha are.
    KgHumidAir,
    /// Per mole of humid air.
    MolHumidAir,
}

/// A moist-air state: humid air at one (T, p, ψ_w).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoistAir {
    t: f64,
    p: f64,
    psi: f64,
    /// The humidity ratio, by the data set's ε.
    w: f64,
    /// Molar volume, m³/mol, and the molar h, s, c_p and c_v, J/mol and J/(mol K).
    v: f64,
    h: f64,
    s: f64,
    cp: f64,
    cv: f64,
    /// (∂p/∂v̄)_T, Pa mol/m³.
    dp_dv: f64,
}

/// v8.0.0's range (`check_bounds`): T from −143.15 to 350 °C, p from 10 Pa to 10 MPa.
pub(crate) const T_MIN: f64 = 130.0;
pub(crate) const T_MAX: f64 = 623.15;
pub(crate) const P_MIN: f64 = 10.0;
pub(crate) const P_MAX: f64 = 10e6;

/// Newton's iterations on the virial equation: from the ideal gas it settles in under ten.
const ITERATIONS: u16 = 50;

/// `t` and `p` within v8.0.0's range and ψ_w within [0, 1).
fn checked(t: f64, p: f64, psi: f64) -> Result<(), Error> {
    if t.is_nan() || t < T_MIN {
        return Err(DomainError::BelowMinTemperature { t, t_min: T_MIN }.into());
    }
    if t > T_MAX {
        return Err(DomainError::AboveMaxTemperature { t, t_max: T_MAX }.into());
    }
    if p.is_nan() || p < P_MIN {
        return Err(DomainError::BelowMinPressure { p, p_min: P_MIN }.into());
    }
    if p > P_MAX {
        return Err(DomainError::AboveMaxPressure { p, p_max: P_MAX }.into());
    }
    if !(0.0..1.0).contains(&psi) {
        return Err(Error::InvalidInput { quantity: WATER_MOLE_FRACTION, value: psi });
    }
    Ok(())
}

/// Σ wᵢ·cᵢ of the coefficients `c` with weights `w`, value and derivatives alike.
fn mix<const N: usize>(w: [f64; N], c: [Coefficient; N]) -> Coefficient {
    let sum = |f: fn(&Coefficient) -> f64| -> f64 { w.iter().zip(&c).map(|(w, c)| w * f(c)).sum() };
    Coefficient { value: sum(|c| c.value), d_dt: sum(|c| c.d_dt), d2_dt2: sum(|c| c.d2_dt2) }
}

/// B_m and C_m at ψ_w = `psi`: (1 − ψ)²B_aa + 2(1 − ψ)ψB_aw + ψ²B_ww and (1 − ψ)³C_aaa + 3(1 − ψ)²ψC_aaw +
/// 3(1 − ψ)ψ²C_aww + ψ³C_www (v8.0.0's `B_m`, `C_m`).
fn mixed(v: &Virials, psi: f64) -> (Coefficient, Coefficient) {
    let x = 1.0 - psi;
    let b = mix([x * x, 2.0 * x * psi, psi * psi], [v.b_aa, v.b_aw, v.b_ww]);
    let c =
        mix([x * x * x, 3.0 * x * x * psi, 3.0 * x * psi * psi, psi * psi * psi], [v.c_aaa, v.c_aaw, v.c_aww, v.c_www]);
    (b, c)
}

/// The larger root of v̄² + 2Bv̄ + 3C, where (∂p/∂v̄)_T of the virial equation vanishes, if it is real and positive:
/// where the equation's stable branch ends.
fn spinodal(b: f64, c: f64) -> Option<f64> {
    let d = b * b - 3.0 * c;
    // A root at v̄ = 0 is no end: its floor is 0, as where there is none, and p there is not finite.
    // equivalent mutant: replace > with >=
    (d >= 0.0).then(|| math::sqrt(d) - b).filter(|v| *v > 0.0)
}

/// The virial equation's pressure RT/v̄ (1 + B/v̄ + C/v̄²) at the molar volume `v`, over (R, T) and (B, C).
pub(crate) fn pressure((r, t): (f64, f64), v: f64, (b, c): (f64, f64)) -> f64 {
    r * t / v * (1.0 + b / v + c / (v * v))
}

/// (∂p/∂v̄)_T of [`pressure`]: −RT/v̄² (1 + 2B/v̄ + 3C/v̄²).
pub(crate) fn dp_dv((r, t): (f64, f64), v: f64, (b, c): (f64, f64)) -> f64 {
    -r * t / (v * v) * (1.0 + 2.0 * b / v + 3.0 * c / (v * v))
}

/// The molar volume at which RT/v̄ (1 + B/v̄ + C/v̄²) is p on the equation's stable branch, by Newton from the ideal
/// gas's to 4 ulp; a step past the branch's end v* halves the way to it instead. A p above p(v*) has no state,
/// `AboveMaxPressure` (v8.0.0's secant returns where it stops after 100 steps); `NoConvergence` if Newton has not
/// settled.
fn molar_volume(r: f64, t: f64, p: f64, b: f64, c: f64) -> Result<f64, Error> {
    let pressure = |v: f64| pressure((r, t), v, (b, c));
    let edge = spinodal(b, c);
    // At p(v*) itself the state is the branch's end, where Newton's slope vanishes: no input reaches it to the bit but
    // by construction, and refusing it is as fair as closing in on it.
    // equivalent mutant: replace > with >=
    if let Some(p_max) = edge.map(pressure).filter(|p_max| p > *p_max) {
        return Err(DomainError::AboveMaxPressure { p, p_max }.into());
    }
    let floor = edge.unwrap_or(0.0);
    // The start only seeds Newton: from any start on the stable branch it closes on the same root to its 4-ulp stop, far
    // inside any state's tolerance, a start left of the end halving its way back.
    // equivalent mutant: replace * with +
    // equivalent mutant: replace * with / #1
    // equivalent mutant: replace / with %
    let mut v = (r * t / p).max(2.0 * floor);
    for _ in 0..ITERATIONS {
        let newton = v - (pressure(v) - p) / dp_dv((r, t), v, (b, c));
        // A Newton step landing on the branch's end itself halves the way to it instead, as one past it does: the
        // next step goes on from there.
        // equivalent mutant: replace > with >=
        let next = if newton > floor { newton } else { (v + floor) / 2.0 };
        if (next - v).abs() <= 4.0 * f64::EPSILON * next {
            return Ok(next);
        }
        v = next;
    }
    Err(Error::NoConvergence { strategy: Strategy::External, iterations: ITERATIONS })
}

/// x ln x, 0 at x = 0.
fn x_ln_x(x: f64) -> f64 {
    if x > 0.0 { x * math::ln(x) } else { 0.0 }
}

/// The moist-air state at (T, p, ψ_w) over `air`'s parts, its ideal parts placed by `zeros`.
pub(crate) fn state(air: &HumidAir, zeros: &Zeros, t: f64, p: f64, psi: f64) -> Result<MoistAir, Error> {
    checked(t, p, psi)?;
    let (parts, x) = (air.parts(), 1.0 - psi);
    let virials = air.virials(t)?;
    let (b, c) = mixed(&virials, psi);
    let v = molar_volume(R, t, p, b.value, c.value)?;
    let v_air = molar_volume(R_AIR, t, p, virials.b_aa.value, virials.c_aaa.value)?;
    let (a, w) = (ideal::air(&parts.air, zeros, t, v_air)?, ideal::water(parts, air.data_set(), zeros, t, p)?);
    let (rt, v2) = (R * t, v * v);
    let h = x * a.h + psi * w.h + rt * ((b.value - t * b.d_dt) / v + (c.value - t * c.d_dt / 2.0) / v2);
    let s_r = (b.value + t * b.d_dt) / v + (c.value + t * c.d_dt) / (2.0 * v2) + x_ln_x(x) + x_ln_x(psi);
    let s = x * a.s + psi * w.s - R * s_r;
    let cp0 = x * a.cp + psi * w.cp;
    let cv_r = (2.0 * b.d_dt + t * b.d2_dt2) / v + (2.0 * c.d_dt + t * c.d2_dt2) / (2.0 * v2);
    let cv = cp0 - R - rt * cv_r;
    let dp_dt = R * (1.0 / v + b.value / v2 + c.value / (v2 * v)) + rt * (b.d_dt / v2 + c.d_dt / (v2 * v));
    let dp_dv = -rt / v2 * (1.0 + 2.0 * b.value / v + 3.0 * c.value / v2);
    let cp = cv - t * dp_dt * dp_dt / dp_dv;
    let w = humidity::ratio(air.data_set(), psi);
    Ok(MoistAir { t, p, psi, w, v, h, s, cp, cv, dp_dv })
}

impl MoistAir {
    /// The temperature, K.
    pub fn t(&self) -> f64 {
        self.t
    }

    /// The pressure, Pa.
    pub fn p(&self) -> f64 {
        self.p
    }

    /// The mole fraction of water ψ_w.
    pub fn mole_fraction(&self) -> f64 {
        self.psi
    }

    /// The humidity ratio W, kg of water per kg of dry air.
    pub fn humidity_ratio(&self) -> f64 {
        self.w
    }

    /// The partial pressure of water vapour, ψ_w·p, Pa.
    pub fn partial_pressure(&self) -> f64 {
        self.psi * self.p
    }

    /// The molar mass of humid air, (1 − ψ_w)M_a + ψ_w·M_w, kg/mol.
    pub fn molar_mass(&self) -> f64 {
        (1.0 - self.psi) * M_AIR + self.psi * M_WATER
    }

    /// A molar quantity `x` per `per`: over M_ha per kilogram of humid air, times (1 + W)/M_ha per kilogram of dry air,
    /// as v8.0.0 converts it (with its ε, `Parity`'s W makes it 1/((1 − ψ_w)M_a) to 5e-7).
    fn per(&self, x: f64, per: Per) -> f64 {
        match per {
            Per::KgDryAir => x * (1.0 + self.w) / self.molar_mass(),
            Per::KgHumidAir => x / self.molar_mass(),
            Per::MolHumidAir => x,
        }
    }

    /// The volume, m³ per `per`.
    pub fn v(&self, per: Per) -> f64 {
        self.per(self.v, per)
    }

    /// The enthalpy, J per `per`.
    pub fn h(&self, per: Per) -> f64 {
        self.per(self.h, per)
    }

    /// The internal energy h − pv, J per `per`.
    pub fn u(&self, per: Per) -> f64 {
        self.per(self.h - self.p * self.v, per)
    }

    /// The entropy, J/K per `per`.
    pub fn s(&self, per: Per) -> f64 {
        self.per(self.s, per)
    }

    /// The isobaric heat capacity, J/K per `per`.
    pub fn cp(&self, per: Per) -> f64 {
        self.per(self.cp, per)
    }

    /// The isochoric heat capacity, J/K per `per`.
    pub fn cv(&self, per: Per) -> f64 {
        self.per(self.cv, per)
    }

    /// The compressibility factor pv̄/(RT).
    pub fn z(&self) -> f64 {
        self.p * self.v / (R * self.t)
    }

    /// The speed of sound, √(−v̄²(c_p/c_v)(∂p/∂v̄)_T/M_ha), m/s.
    pub fn speed_of_sound(&self) -> f64 {
        math::sqrt(-self.v * self.v * self.cp / self.cv * self.dp_dv / self.molar_mass())
    }

    /// The isentropic exponent −(c_p/c_v)(v̄/p)(∂p/∂v̄)_T.
    pub fn isentropic_exponent(&self) -> f64 {
        -self.cp / self.cv * self.v / self.p * self.dp_dv
    }
}
