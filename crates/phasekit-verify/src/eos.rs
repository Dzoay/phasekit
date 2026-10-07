//! Checking `eos` fixtures (VERIFICATION.md §3.5) against a fluid's DT flash with the oracle's phase imposed, shared by
//! the corpus tests and the nightly sweep: every output under class `Prop` (VERIFICATION.md §5), 1e-12 of max(|value|,
//! floor) (1e-8 in the near-critical window), or the `Term` bound of the α derivatives carried through the output's
//! relation where that is larger.

use phasekit_core::internal::FluidRecord;
use phasekit_core::{
    Basis, Bundle, Density, DerivVar, DomainPolicy, Error, FlashOptions, Fluid, Input, Order, Partial, Phase, State,
    Temperature, math,
};

use crate::term::{IdealScale, report};
use crate::{CheckError, Fixture, ToleranceClass, Window, majorant};

/// What an output's `Prop` scale is floored at (VERIFICATION.md §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Floor {
    /// No floor: the value itself (p, w, Z).
    Value,
    /// A molar energy: R·T.
    Energy,
    /// An entropy or heat capacity: R.
    Entropy,
    /// A first partial ∂p/∂Y: |p|/|Y| at the state.
    PressureOver(DerivVar),
}

/// The property columns of an `eos` fixture compared since M5.1, with their floors.
pub const COLUMNS: [(&str, Floor); 10] = [
    ("p", Floor::Value),
    ("hmolar", Floor::Energy),
    ("smolar", Floor::Entropy),
    ("umolar", Floor::Energy),
    ("cvmolar", Floor::Entropy),
    ("cpmolar", Floor::Entropy),
    ("speed_sound", Floor::Value),
    ("Z", Floor::Value),
    ("dpdrho_T", Floor::PressureOver(DerivVar::Dmolar)),
    ("dpdT_rho", Floor::PressureOver(DerivVar::T)),
];

/// The `Prop` scale max(|want|, floor) of an output whose oracle value is `want`, at (T, ρ) where the oracle's pressure
/// is `p`; `r` is the EOS's gas constant.
pub fn scale(floor: Floor, want: f64, r: f64, t: f64, rho: f64, p: f64) -> f64 {
    let floor = match floor {
        Floor::Value => 0.0,
        Floor::Energy => r * t,
        Floor::Entropy => r,
        Floor::PressureOver(DerivVar::T) => p.abs() / t,
        Floor::PressureOver(_) => p.abs() / rho,
    };
    want.abs().max(floor)
}

/// The `Term` scales M_ij = Σ_k |φ_k| of the order-2 bundle entries A_ij at one (T, ρ) (VERIFICATION.md §5): the
/// residual part's [`majorant::eos`] plus the ideal part's [`IdealScale`] (1 for the exact A01⁰ and A02⁰).
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)] // M_ij of A_ij
pub struct Majorants {
    pub m00: f64,
    pub m10: f64,
    pub m01: f64,
    pub m20: f64,
    pub m11: f64,
    pub m02: f64,
}

impl Majorants {
    /// The scales of `record`'s bundle at (T, ρ); `ideal` is [`IdealScale::new`] of the same record.
    pub fn at(record: &FluidRecord, ideal: &IdealScale, t: f64, rho: f64) -> Majorants {
        let e = &record.eos;
        let (tau, delta) = (e.t_reducing / t, rho / e.rho_reducing);
        let m = |i, j| majorant::eos(e, tau, delta, i, j) + ideal.get(t, rho, i, j);
        Majorants { m00: m(0, 0), m10: m(1, 0), m01: m(0, 1), m20: m(2, 0), m11: m(1, 1), m02: m(0, 2) }
    }
}

/// Σ_ij |∂X/∂A_ij|·M_ij of output `column` at the bundle `b`: the `Term` scale of every entry carried through the
/// output's relation to first order, so `Term`'s bound on it is how far two evaluations that each meet `Term` may
/// differ in X (VERIFICATION.md §5). With x = A01 − A11 and D = 2·A01 + A02: cp = cv + R·x²/D, w² = RT·(D +
/// x²/(−A20))/M. NaN for a column it does not know and for w where w² ≤ 0.
pub fn carried(column: &str, b: &Bundle, m: &Majorants, r: f64, t: f64, rho: f64, molar_mass: f64) -> f64 {
    let (x, d) = (b.a01 - b.a11, 2.0 * b.a01 + b.a02);
    let (m_x, m_d) = (m.m01 + m.m11, 2.0 * m.m01 + m.m02);
    match column {
        "p" => rho * r * t * m.m01,
        "hmolar" => r * t * (m.m10 + m.m01),
        "smolar" => r * (m.m10 + m.m00),
        "umolar" => r * t * m.m10,
        "cvmolar" => r * m.m20,
        "cpmolar" => r * (m.m20 + 2.0 * (x / d).abs() * m_x + (x / d) * (x / d) * m_d),
        "speed_sound" => {
            let (rt_m, xa) = (r * t / molar_mass, x / b.a20);
            let w = math::sqrt(rt_m * (d - x * xa));
            rt_m * (m_d + 2.0 * xa.abs() * m_x + xa * xa * m.m20) / (2.0 * w)
        }
        "Z" => m.m01,
        "dpdrho_T" => r * t * m_d,
        "dpdT_rho" => rho * r * m_x,
        _ => f64::NAN,
    }
}

/// The options of every `eos` row: the oracle's imposed phase (gas, which skips the phase rule) and `Extrapolate`,
/// because the grid reaches pressures above `pmax` that the oracle evaluates unchecked; such states come back flagged.
pub fn options() -> FlashOptions {
    FlashOptions::new().with_phase(Phase::Gas).with_domain(DomainPolicy::Extrapolate)
}

/// The DT state of a row: (T, ρ) in mol/m³, the phase imposed as the oracle imposes it.
pub fn state(fluid: &Fluid, t: f64, rho: f64) -> Result<State, Error> {
    fluid.flash(Input::dt(Density::molar(rho)?, Temperature::new(t)?), &options())
}

/// `column` of `state`; an output the state refuses (w where (∂p/∂ρ)_T < 0) is NaN, as the oracle writes it.
pub fn output(state: &State, column: &str) -> f64 {
    let partial = |of, wrt, at| state.partial(Partial { of, wrt, at });
    let value = match column {
        "p" => Ok(state.p()),
        "hmolar" => Ok(state.h(Basis::Molar)),
        "smolar" => Ok(state.s(Basis::Molar)),
        "umolar" => Ok(state.u(Basis::Molar)),
        "cvmolar" => state.cv(Basis::Molar),
        "cpmolar" => state.cp(Basis::Molar),
        "speed_sound" => state.speed_of_sound(),
        "Z" => Ok(state.z()),
        "dpdrho_T" => partial(DerivVar::P, DerivVar::Dmolar, DerivVar::T),
        "dpdT_rho" => partial(DerivVar::P, DerivVar::T, DerivVar::Dmolar),
        _ => Ok(f64::NAN),
    };
    value.unwrap_or(f64::NAN)
}

/// The running result of an `eos` check.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EosCheck {
    /// Entries compared.
    pub checked: usize,
    /// One line per entry outside `Prop` (or row that could not be evaluated), as map 10 U1 asks.
    pub failures: Vec<String>,
    /// The largest error / bound of the entries that passed.
    pub headroom: f64,
}

impl EosCheck {
    /// Checks `rows` (0-based) of `fixture` against `fluid`, whose decoded data is `record`. Every row's status must be
    /// `ok`; the oracle's `p` sets the partial-derivative floors.
    pub fn rows(&mut self, fixture: &Fixture<'_>, rows: &[usize], fluid: &Fluid, record: &FluidRecord) {
        let (Some(eos), Ok(ideal)) = (fluid.model().helmholtz(), IdealScale::new(record)) else {
            self.failures.push(format!("{}: not a Helmholtz fluid", record.name));
            return;
        };
        let (r, molar_mass) = (eos.gas_constant(), fluid.info().molar_mass());
        let critical = fluid.model().critical_point();
        for &row in rows {
            let number = |column: &str| fixture.value(row, column);
            let (Some(t), Some(rho), Some(p)) = (number("T"), number("rhomolar"), number("p")) else {
                self.failures.push(format!("row {row}: T, rhomolar or p is missing (status not ok?)"));
                continue;
            };
            let (state, bundle) = match (
                state(fluid, t, rho),
                (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle(),
            ) {
                (Ok(state), Some(bundle)) => (state, bundle),
                (state, _) => {
                    self.failures.push(format!("row {row} (T = {t}, ρ = {rho}): the flash failed: {:?}", state.err()));
                    continue;
                }
            };
            let majorants = Majorants::at(record, &ideal, t, rho);
            let window = critical.map_or(Window::Regular, |c| Window::at(t, rho, c.t, c.rho));
            for (column, floor) in COLUMNS {
                let Some(want) = number(column) else { continue };
                let carried = carried(column, &bundle, &majorants, r, t, rho, molar_mass);
                let bound = ToleranceClass::Prop.bound_carried(scale(floor, want, r, t, rho, p), window, carried);
                self.checked += 1;
                match fixture.check_bound(row, column, output(&state, column), bound.unwrap_or(0.0)) {
                    Ok(ratio) => self.headroom = self.headroom.max(ratio),
                    Err(CheckError::Mismatch(m)) => self.failures.push(m.to_string()),
                    Err(e) => self.failures.push(format!("row {row}, {column}: {e:?}")),
                }
            }
        }
    }

    /// `None` when every entry passed, else a report of the first `shown` failures.
    pub fn report(&self, shown: usize) -> Option<String> {
        report(&self.failures, self.checked, self.headroom, "Prop", shown)
    }
}

/// The running result of the L4 identities (VERIFICATION.md §8.1), class `Identity`: |lhs − rhs| within 1e-12 (1e-8 in
/// the near-critical window) of the largest term of the identity, every summand of a first partial's Jacobian
/// expansion counted as a term.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IdentityCheck {
    /// Identities evaluated.
    pub checked: usize,
    /// One line per identity outside `Identity`.
    pub failures: Vec<String>,
    /// The largest error / bound of the identities that held.
    pub headroom: f64,
}

impl IdentityCheck {
    /// Checks one identity: `lhs = rhs` with the magnitudes `terms` of its summands.
    pub fn check(&mut self, at: &str, name: &str, (lhs, rhs): (f64, f64), terms: &[f64], window: Window) {
        let largest = terms.iter().fold(lhs.abs().max(rhs.abs()), |m, x| m.max(x.abs()));
        let bound = ToleranceClass::Identity.bound_in(largest, window).unwrap_or(0.0);
        let error = (lhs - rhs).abs();
        self.checked += 1;
        if error <= bound {
            self.headroom = self.headroom.max(error / bound);
        } else {
            self.failures.push(format!("{at}: {name}: {lhs:?} against {rhs:?}, error {error:e} > {bound:e}"));
        }
    }

    /// `None` when every identity held, else a report of the first `shown` failures.
    pub fn report(&self, shown: usize) -> Option<String> {
        report(&self.failures, self.checked, self.headroom, "Identity", shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each floor of VERIFICATION.md §5 as a number (R = 8, T = 300, ρ = 1000, p = 2e6), and |want| where that is
    /// larger.
    #[test]
    fn scales_are_the_larger_of_value_and_floor() {
        let floors: [(Floor, f64); 5] = [
            (Floor::Value, 0.0),
            (Floor::Energy, 2400.0),
            (Floor::Entropy, 8.0),
            (Floor::PressureOver(DerivVar::T), 2e6 / 300.0),
            (Floor::PressureOver(DerivVar::Dmolar), 2000.0),
        ];
        for (floor, value) in floors {
            assert_eq!(scale(floor, 1e-3, 8.0, 300.0, 1e3, -2e6), value.max(1e-3), "{floor:?}");
            assert_eq!(scale(floor, -1e9, 8.0, 300.0, 1e3, 2e6), 1e9, "{floor:?}");
        }
    }

    /// Every relation's carried scale by hand, at a bundle with x = A01 − A11 = 1, D = 2·A01 + A02 = 2, A20 = −4 and
    /// distinct M_ij (R = 2, T = 3, RT = 6, ρ = 5, M = 0.5): w² = RT·(D + x²/4)/M = 27.
    #[test]
    fn carried_scales_follow_the_relations() {
        let b = Bundle { a00: 0.5, a10: 1.5, a01: 3.0, a20: -4.0, a11: 2.0, a02: -4.0 };
        let m = Majorants { m00: 1.0, m10: 2.0, m01: 3.0, m20: 5.0, m11: 7.0, m02: 11.0 };
        let (m_x, m_d) = (3.0 + 7.0, 6.0 + 11.0);
        let expected = [
            ("p", 5.0 * 6.0 * 3.0),
            ("hmolar", 6.0 * (2.0 + 3.0)),
            ("smolar", 2.0 * (2.0 + 1.0)),
            ("umolar", 6.0 * 2.0),
            ("cvmolar", 2.0 * 5.0),
            ("cpmolar", 2.0 * (5.0 + 2.0 * 0.5 * m_x + 0.25 * m_d)),
            ("speed_sound", 12.0 * (m_d + 2.0 * 0.25 * m_x + 0.0625 * 5.0) / (2.0 * math::sqrt(27.0))),
            ("Z", 3.0),
            ("dpdrho_T", 6.0 * m_d),
            ("dpdT_rho", 5.0 * 2.0 * m_x),
        ];
        for (column, want) in expected {
            let got = carried(column, &b, &m, 2.0, 3.0, 5.0, 0.5);
            assert!((got - want).abs() <= 1e-15 * want, "{column}: {got} against {want}");
        }
        assert!(carried("T", &b, &m, 2.0, 3.0, 5.0, 0.5).is_nan());
        let unstable = Bundle { a02: -9.0, ..b }; // D = −3: w² < 0
        assert!(carried("speed_sound", &unstable, &m, 2.0, 3.0, 5.0, 0.5).is_nan());
    }

    /// An identity holds up to and including its bound, 1e-12 of its largest term (1e-8 near critical), and the
    /// headroom is the largest error / bound that held.
    #[test]
    fn identity_bound_is_inclusive_and_scaled_by_the_largest_term() {
        let mut check = IdentityCheck::default();
        check.check("a", "x = y", (0.0, 2e-12), &[2.0, -1.0], Window::Regular);
        check.check("a", "x = y", (0.0, 1e-12), &[-4.0], Window::Regular);
        assert_eq!((check.checked, check.failures.len(), check.headroom), (2, 0, 1.0));
        check.check("b", "x = y", (0.0, 2.1e-12), &[2.0], Window::Regular);
        check.check("c", "x = y", (3.0, 3.0 + 2e-8), &[], Window::NearCritical);
        check.check("d", "x = y", (3.0, 3.0 + 4e-8), &[], Window::NearCritical);
        assert_eq!(check.checked, 5);
        assert_eq!(check.failures.len(), 2, "{:?}", check.failures);
        assert!(check.failures[0].starts_with("b: x = y: 0.0 against 2.1e-12"), "{}", check.failures[0]);
        assert!(check.failures[1].starts_with("d: "), "{}", check.failures[1]);
        assert_eq!(check.report(1).map(|r| r.lines().count()), Some(2));
    }
}
