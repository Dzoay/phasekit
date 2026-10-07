//! Checking `flash` fixtures (VERIFICATION.md §3.5, §8.2), shared by the corpus tests and the nightly sweep. Since M5.3
//! the DT rows: (ρ, T) read off a truth state, flashed with no phase imposed, against CoolProp's flash of the same
//! inputs. The phase label and Q (class `Flash`, 1e-8 absolute) must match, and p, h, s, u within class `Flash` (1e-9 of
//! max(|v|, floor)). A single-phase DT state is the EOS relation at its input (T, ρ), so the `Term` bound carried
//! through that relation applies to it too (user decision TC1).

use phasekit_core::internal::FluidRecord;
use phasekit_core::{Basis, Density, Error, Fluid, Input, Order, Pair, Phase, Temperature};

use crate::eos::{Majorants, carried};
use crate::term::{IdealScale, report};
use crate::{Cell, Fixture, ToleranceClass, Window};

/// CoolProp's name of a phase.
pub fn coolprop_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Liquid => "liquid",
        Phase::Gas => "gas",
        Phase::TwoPhase => "twophase",
        Phase::Supercritical => "supercritical",
        Phase::SupercriticalGas => "supercritical_gas",
        Phase::SupercriticalLiquid => "supercritical_liquid",
        Phase::CriticalPoint => "critical_point",
        _ => "other",
    }
}

/// The running result of a `flash` check.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlashCheck {
    /// Rows whose state was compared.
    pub compared: usize,
    /// Of those, the two-phase ones.
    pub two_phase: usize,
    /// Rows refused as expected: a pseudo-pure fluid below its critical temperature has no saturation curve before M6.
    pub unsupported: usize,
    /// One line per disagreement.
    pub failures: Vec<String>,
}

impl FlashCheck {
    /// Checks every DT row of `fixture` against `fluid`, whose decoded data is `record`. Every row's status is `ok`.
    pub fn dt_rows(&mut self, fixture: &Fixture<'_>, fluid: &Fluid, record: &FluidRecord) {
        let path = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap_or_default();
        let (Some(eos), Ok(ideal), Some(critical)) =
            (fluid.model().helmholtz(), IdealScale::new(record), record.critical)
        else {
            self.failures.push(format!("{path}: not a Helmholtz fluid with a critical point"));
            return;
        };
        let (pseudo_pure, r, m) = (record.superancillary.is_none(), record.eos.gas_constant, fluid.info().molar_mass());
        let label = |row: usize, column: &str| {
            let i = fixture.columns().iter().position(|c| *c == column);
            match i.and_then(|i| fixture.rows().get(row)?.cells.get(i).copied()) {
                Some(Cell::Text(text)) => text,
                _ => "",
            }
        };
        for row in 0..fixture.rows().len() {
            let number = |column: &str| fixture.value(row, column).unwrap_or(f64::NAN);
            let (rho, t) = (number("x1"), number("x2"));
            if (label(row, "pair"), label(row, "status")) != ("DT", "ok") {
                self.failures.push(format!("{path} row {row}: not an ok DT row"));
                continue;
            }
            let result = Density::molar(rho)
                .and_then(|d| Ok(Input::dt(d, Temperature::new(t)?)))
                .and_then(|input| fluid.state(input));
            if pseudo_pure && t < critical.t {
                match result {
                    Err(Error::Unsupported { pair: Pair::DT }) => self.unsupported += 1,
                    other => self.failures.push(format!("{path} row {row}: {other:?}, expected Unsupported")),
                }
                continue;
            }
            let state = match result {
                Ok(state) => state,
                Err(e) => {
                    self.failures.push(format!("{path} row {row} ({rho} mol/m³, {t} K): {e:?}"));
                    continue;
                }
            };
            self.compared += 1;
            if label(row, "phase") != coolprop_name(state.phase()) {
                self.failures.push(format!("{path} row {row}: {:?}, oracle {}", state.phase(), label(row, "phase")));
            }
            let single = state.quality().is_none();
            let bundle = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle();
            let majorants = Majorants::at(record, &ideal, t, rho);
            let b = Basis::Molar;
            let columns = [
                ("p", state.p(), 0.0, "p"),
                ("h", state.h(b), r * t, "hmolar"),
                ("s", state.s(b), r, "smolar"),
                ("u", state.u(b), r * t, "umolar"),
            ];
            for (column, got, floor, relation) in columns {
                let want = number(column);
                let spread = match (single, &bundle) {
                    (true, Some(bundle)) => carried(relation, bundle, &majorants, r, t, rho, m),
                    _ => f64::NAN,
                };
                let bound = ToleranceClass::Flash.bound_carried(want.abs().max(floor), Window::Regular, spread);
                if let Err(e) = fixture.check_bound(row, column, got, bound.unwrap_or(0.0)) {
                    self.failures.push(format!("{e:?}"));
                }
            }
            let q = number("Q");
            match state.quality() {
                Some(got) if (got - q).abs() <= ToleranceClass::FLASH_QUALITY => self.two_phase += 1,
                None if q.is_nan() => {}
                got => self.failures.push(format!("{path} row {row}: Q {got:?}, oracle {q}")),
            }
        }
    }

    /// `None` when every row agreed, else the first `shown` disagreements.
    pub fn report(&self, shown: usize) -> Option<String> {
        report(&self.failures, self.compared + self.unsupported, 0.0, "Flash", shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phasekit_core::{DataSet, Registry, State};

    /// Hand-made DT rows in a `flash/v1` file (source `mp:` only so that no oracle header is required).
    fn file(fluid: &str, rows: &[String]) -> String {
        let head = "# fixture: flash/v1\n# fluid: FLUID\n# source: mp:hand-made rows\n\
                    # columns: pair,x1,x2,truth,status,T,rho,p,h,s,u,Q,phase\n\
                    # units: -,mol/m3,K,-,-,K,mol/m3,Pa,J/mol,J/mol/K,J/mol,-,-\n\
                    # tol: label,in,in,label,label,flash,flash,flash,flash,flash,flash,flash,label\n";
        format!("{}{}\n", head.replace("FLUID", fluid), rows.join("\n"))
    }

    /// The row of `state` as the oracle would write it, with h and u moved by `dh` and Q set to `q`.
    fn row(state: &State, dh: f64, q: f64) -> String {
        let b = Basis::Molar;
        let cells = [state.rho(b), state.t(), state.p(), state.h(b) + dh, state.s(b), state.u(b) + dh, q];
        let [rho, t, p, h, s, u, q] = cells.map(|v| if v.is_nan() { "nan".to_string() } else { format!("{v:?}") });
        format!("DT,{rho},{t},QT,ok,{t},{rho},{p},{h},{s},{u},{q},{}", coolprop_name(state.phase()))
    }

    /// `rows` of `name` through `dt_rows`, against the `Parity` data.
    fn check(name: &str, rows: &[String]) -> FlashCheck {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let text = file(name, rows);
        let fixture = Fixture::parse("hand-made", &text).unwrap();
        let mut check = FlashCheck::default();
        check.dt_rows(&fixture, registry.get(name).unwrap(), &record);
        check
    }

    /// What `dt_rows` (M5.3) accepts and refuses, on rows built around real states:
    /// - Water's liquid by its reference state, where |h| and |u| are far below R·T, so the floor R·T sets their
    ///   bound (`Flash`, 1e-9 of it, 2.27e-6 J/mol; the bound carried through h's relation is about 1.4e-6 there):
    ///   2e-6 J/mol off is within, 2e-5 is not;
    /// - a two-phase state whose Q is 1e-6 off, and a single phase where the oracle has a Q, are failures;
    /// - a pseudo-pure fluid (R410A) is compared from its critical temperature up, at T_c itself too, and is expected
    ///   `Unsupported` below it.
    #[test]
    fn dt_rows_floors_qualities_and_the_pseudo_pure_boundary() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let state = |name: &str, rho: f64, t: f64| {
            let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
            registry.get(name).unwrap().state(input).unwrap()
        };
        let liquid = state("Water", 55_500.0, 273.16);
        assert!(liquid.h(Basis::Molar).abs() < 10.0 && liquid.quality().is_none(), "{}", liquid.h(Basis::Molar));
        let within = check("Water", &[row(&liquid, 2e-6, f64::NAN)]);
        assert_eq!((within.compared, within.failures.len()), (1, 0), "{:?}", within.failures);
        assert_eq!(check("Water", &[row(&liquid, 2e-5, f64::NAN)]).failures.len(), 2, "h and u");
        assert_eq!(check("Water", &[row(&liquid, 0.0, 0.5)]).failures.len(), 1, "a single phase with a Q");
        let wet = state("Water", 1_000.0, 400.0);
        let q = wet.quality().unwrap();
        let two_phase = check("Water", &[row(&wet, 0.0, q)]);
        assert_eq!((two_phase.two_phase, two_phase.failures.len()), (1, 0), "{:?}", two_phase.failures);
        assert_eq!(check("Water", &[row(&wet, 0.0, q + 1e-6)]).failures.len(), 1, "Q 1e-6 off");
        let record = phasekit_core::internal::record(&registry, "R410A").unwrap();
        let critical = record.critical.unwrap();
        let at_tc = check("R410A", &[row(&state("R410A", critical.rho / 2.0, critical.t), 0.0, f64::NAN)]);
        assert_eq!((at_tc.compared, at_tc.unsupported, at_tc.failures.len()), (1, 0, 0), "{:?}", at_tc.failures);
        let below = format!("DT,{:?},{:?},QT,ok,{:?},{:?},1.0,1.0,1.0,1.0,nan,gas", 100.0, 300.0, 300.0, 100.0);
        assert_eq!(check("R410A", &[below]).unsupported, 1);
    }

    /// CoolProp's phase names, one per label.
    #[test]
    fn phases_have_coolprop_names() {
        let names = [
            Phase::Liquid,
            Phase::Gas,
            Phase::TwoPhase,
            Phase::Supercritical,
            Phase::SupercriticalGas,
            Phase::SupercriticalLiquid,
            Phase::CriticalPoint,
            Phase::Solid,
        ]
        .map(coolprop_name);
        assert_eq!(
            names,
            [
                "liquid",
                "gas",
                "twophase",
                "supercritical",
                "supercritical_gas",
                "supercritical_liquid",
                "critical_point",
                "other"
            ]
        );
        let check = FlashCheck { compared: 3, unsupported: 1, failures: vec!["x".into()], ..Default::default() };
        assert_eq!(check.report(1), Some("1 of 4 entries outside Flash (headroom 0.000):\nx".into()));
        assert_eq!(FlashCheck::default().report(1), None);
    }
}
