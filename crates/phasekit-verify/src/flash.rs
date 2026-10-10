//! Checking `flash` fixtures (VERIFICATION.md §3.5, §8.2), shared by the corpus tests and the nightly sweep: a pair's
//! inputs read off a truth state, flashed with no phase imposed, against CoolProp's flash of the same inputs. The DT rows
//! since M5.3, (ρ, T); the PT rows since M7.1, the truth's own (p, T), whose ρ is an output too. The phase label and Q
//! (class `Flash`, 1e-8 absolute) must match, and ρ (PT), p, h, s, u within class `Flash` (1e-9 of max(|v|, floor)). A
//! single-phase state is the EOS relation at its (T, ρ), so the `Term` bound carried through that relation applies to
//! it too (user decision TC1).

use phasekit_core::internal::FluidRecord;
use phasekit_core::{Basis, Density, Fluid, Input, Order, Phase, Pressure, Temperature};

use crate::eos::{Majorants, carried};
use crate::fixture::Kind;
use crate::register::{DIVERGENCES, RowKey, exempt_row};
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

/// Whether `got` lies further than `bound` from `want`.
fn beyond(got: f64, want: f64, bound: f64) -> bool {
    (got - want).abs() > bound
}

/// The running result of a `flash` check.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlashCheck {
    /// Rows whose state was compared.
    pub compared: usize,
    /// Of those, the two-phase ones.
    pub two_phase: usize,
    /// Cells the register exempts (`register::exempt_row`, kind `flash`).
    pub exempt: usize,
    /// One line per disagreement.
    pub failures: Vec<String>,
}

impl FlashCheck {
    /// Checks every DT row of `fixture` against `fluid`, whose decoded data is `record`. Every row's status is `ok`.
    pub fn dt_rows(&mut self, fixture: &Fixture<'_>, fluid: &Fluid, record: &FluidRecord) {
        self.pair_rows(fixture, "DT", fluid, record);
    }

    /// Checks every PT row of `fixture` as [`Self::dt_rows`] does, ρ among the outputs (PLAN.md M7.1).
    pub fn pt_rows(&mut self, fixture: &Fixture<'_>, fluid: &Fluid, record: &FluidRecord) {
        self.pair_rows(fixture, "PT", fluid, record);
    }

    /// The rows of `pair` in a one-fluid file.
    fn pair_rows(&mut self, fixture: &Fixture<'_>, pair: &str, fluid: &Fluid, record: &FluidRecord) {
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap_or_default();
        self.rows_where(fixture, name, (fluid, record), |row| fixture.printed(row, "pair") == Some(pair));
    }

    /// Checks the rows `keep` selects of `name` (one fluid's in an all-fluid file), DT and PT alike.
    pub fn dt_rows_where(
        &mut self,
        fixture: &Fixture<'_>,
        name: &str,
        (fluid, record): (&Fluid, &FluidRecord),
        keep: impl Fn(usize) -> bool,
    ) {
        self.rows_where(fixture, name, (fluid, record), keep);
    }

    /// [`Self::dt_rows_where`]: each row's input by its pair.
    fn rows_where(
        &mut self,
        fixture: &Fixture<'_>,
        name: &str,
        (fluid, record): (&Fluid, &FluidRecord),
        keep: impl Fn(usize) -> bool,
    ) {
        let path = name;
        let (Some(eos), Ok(ideal)) = (fluid.model().helmholtz(), IdealScale::new(record)) else {
            self.failures.push(format!("{path}: not a Helmholtz fluid"));
            return;
        };
        let (r, m) = (record.eos.gas_constant, fluid.info().molar_mass());
        let label = |row: usize, column: &str| {
            let i = fixture.columns().iter().position(|c| *c == column);
            match i.and_then(|i| fixture.rows().get(row)?.cells.get(i).copied()) {
                Some(Cell::Text(text)) => text,
                _ => "",
            }
        };
        for row in (0..fixture.rows().len()).filter(|&row| keep(row)) {
            let number = |column: &str| fixture.value(row, column).unwrap_or(f64::NAN);
            let (x1, t) = (number("x1"), number("x2"));
            let input = match (label(row, "pair"), label(row, "status")) {
                ("DT", "ok") => Density::molar(x1).and_then(|d| Ok(Input::dt(d, Temperature::new(t)?))),
                ("PT", "ok") => Pressure::new(x1).and_then(|p| Ok(Input::pt(p, Temperature::new(t)?))),
                _ => {
                    self.failures.push(format!("{path} row {row}: not an ok DT or PT row"));
                    continue;
                }
            };
            let state = match input.and_then(|input| fluid.state(input)) {
                Ok(state) => state,
                Err(e) => {
                    self.failures.push(format!("{path} row {row} ({} {x1}, {t} K): {e:?}", label(row, "pair")));
                    continue;
                }
            };
            let rho = state.rho(Basis::Molar);
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
            if label(row, "pair") == "PT" {
                let bound = ToleranceClass::Flash.bound(number("rho").abs());
                if let Err(e) = fixture.check_bound(row, "rho", rho, bound.unwrap_or(0.0)) {
                    self.failures.push(format!("{e:?}"));
                }
            }
            for (column, got, floor, relation) in columns {
                let key = RowKey { t, input: Some(label(row, "pair")), two_phase: !single };
                if exempt_row(DIVERGENCES, name, Kind::Flash, column, key).is_some() {
                    self.exempt += 1;
                    continue;
                }
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

    /// PLAN.md M7.1, class `Flash`: PT of `name` at (p, T) round-trips. DT at the state's (ρ, T) gives the same phase,
    /// and the state's p is the input within `Flash` (1e-9 of p) or the `Term` bound carried through p's relation where
    /// that is larger (user decision TC1: a liquid at 1 Pa holds p only to the rounding of terms near ρRT). A refused
    /// point is returned for the caller to judge, uncounted.
    pub fn pt_round_trip(
        &mut self,
        name: &str,
        (fluid, record): (&Fluid, &FluidRecord),
        (p, t): (f64, f64),
    ) -> Result<(), phasekit_core::Error> {
        let ideal = IdealScale::new(record)?;
        let input = Pressure::new(p).and_then(|pressure| Ok(Input::pt(pressure, Temperature::new(t)?)));
        let state = input.and_then(|input| fluid.state(input))?;
        self.compared += 1;
        let rho = state.rho(Basis::Molar);
        let back =
            Density::molar(rho).and_then(|d| Ok(Input::dt(d, Temperature::new(t)?))).and_then(|i| fluid.state(i));
        if back.as_ref().map(phasekit_core::State::phase) != Ok(state.phase()) {
            self.failures.push(format!("{name} PT({p} Pa, {t} K) {:?} at ρ = {rho}: DT gives {back:?}", state.phase()));
        }
        let (r, m) = (record.eos.gas_constant, fluid.info().molar_mass());
        let bundle = fluid
            .model()
            .helmholtz()
            .and_then(|e| (e.ideal(t, rho, Order::Two) + e.residual(t, rho, Order::Two)).bundle());
        let spread =
            bundle.map_or(f64::NAN, |b| carried("p", &b, &Majorants::at(record, &ideal, t, rho), r, t, rho, m));
        let bound = ToleranceClass::Flash.bound_carried(p, Window::Regular, spread).unwrap_or(0.0);
        if beyond(state.p(), p, bound) {
            self.failures.push(format!("{name} PT({p} Pa, {t} K): p {} beyond {bound}", state.p()));
        }
        Ok(())
    }

    /// `None` when every row agreed, else the first `shown` disagreements.
    pub fn report(&self, shown: usize) -> Option<String> {
        report(&self.failures, self.compared, 0.0, "Flash", shown)
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

    /// `state`'s row with p moved by the factor `scale`.
    fn row_with_p(state: &State, scale: f64) -> String {
        let row = row(state, 0.0, state.quality().unwrap_or(f64::NAN));
        let mut cells: Vec<String> = row.split(',').map(str::to_owned).collect();
        cells[7] = format!("{:?}", state.p() * scale);
        cells.join(",")
    }

    /// `rows` of `name`, DT and PT alike, against the `Parity` data.
    fn check(name: &str, rows: &[String]) -> FlashCheck {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let text = file(name, rows);
        let fixture = Fixture::parse("hand-made", &text).unwrap();
        let mut check = FlashCheck::default();
        check.dt_rows_where(&fixture, name, (registry.get(name).unwrap(), &record), |_| true);
        check
    }

    /// What `dt_rows` (M5.3) accepts and refuses, on rows built around real states:
    /// - Water's liquid by its reference state, where |h| and |u| are far below R·T, so the floor R·T sets their
    ///   bound (`Flash`, 1e-9 of it, 2.27e-6 J/mol; the bound carried through h's relation is about 1.4e-6 there):
    ///   2e-6 J/mol off is within, 2e-5 is not;
    /// - a two-phase state whose Q is 1e-6 off, and a single phase where the oracle has a Q, are failures;
    /// - a pseudo-pure fluid (R410A) is compared at its critical temperature and, since M6.9, inside its dome below it,
    ///   whose state is the pure VLE of its EOS;
    /// - only the rows a filter keeps are checked.
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
        assert_eq!((at_tc.compared, at_tc.failures.len()), (1, 0), "{:?}", at_tc.failures);
        let dome = state("R410A", 3_000.0, 280.0);
        let in_dome = check("R410A", &[row(&dome, 0.0, dome.quality().unwrap())]);
        assert_eq!((in_dome.two_phase, in_dome.failures.len()), (1, 0), "{:?}", in_dome.failures);
        let text = file("Water", &[row(&wet, 1.0, q)]);
        let fixture = Fixture::parse("hand-made", &text).unwrap();
        let (mut none, water) = (FlashCheck::default(), phasekit_core::internal::record(&registry, "Water").unwrap());
        none.dt_rows_where(&fixture, "Water", (registry.get("Water").unwrap(), &water), |_| false);
        assert_eq!((none.compared, none.failures.len()), (0, 0), "a row filtered out is not checked");
    }

    /// DIV-0019 through the register: SES36's two-phase row at 206.2675 K with p 1e-8 off is exempt in p (counted, no
    /// failure); a single-phase row there with the same misfit is not.
    #[test]
    fn exempt_cells_are_counted_not_compared() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let ses36 = registry.get("SES36").unwrap();
        let state = |rho: f64| {
            ses36.state(Input::dt(Density::molar(rho).unwrap(), Temperature::new(206.2675).unwrap())).unwrap()
        };
        let (dome, gas) = (state(1.97058), state(0.05));
        assert_eq!((dome.phase(), gas.phase()), (Phase::TwoPhase, Phase::Gas));
        let exempt = check("SES36", &[row_with_p(&dome, 1.0 + 1e-8)]);
        assert_eq!((exempt.exempt, exempt.failures.len()), (1, 0), "{:?}", exempt.failures);
        let single = check("SES36", &[row_with_p(&gas, 1.0 + 1e-8)]);
        assert_eq!((single.exempt, single.failures.len()), (0, 1), "{:?}", single.failures);
    }

    /// PT rows (PLAN.md M7.1): ρ is an output there, compared within `Flash`, and p, h, s and u are DIV-0020's, counted
    /// as exempt: Water's PT state at 1 MPa and 500 K agrees with its own row; its row with ρ 1e-6 off is a failure, and
    /// one with h 1e-6 off is not.
    #[test]
    fn pt_rows_compare_rho_and_exempt_the_stale_outputs() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let water = registry.get("Water").unwrap();
        let pt = Input::pt(Pressure::new(1e6).unwrap(), Temperature::new(500.0).unwrap());
        let state = water.state(pt).unwrap();
        let b = Basis::Molar;
        let row = |rho: f64, dh: f64| {
            let (p, t) = (1e6_f64, 500.0_f64);
            let (h, s, u) = (state.h(b) + dh, state.s(b), state.u(b));
            format!("PT,{p:?},{t:?},PT,ok,{t:?},{rho:?},{p:?},{h:?},{s:?},{u:?},nan,{}", coolprop_name(state.phase()))
        };
        let rho = state.rho(b);
        let exact = check("Water", &[row(rho, 0.0)]);
        assert_eq!((exact.compared, exact.exempt, exact.failures.len()), (1, 4, 0), "{:?}", exact.failures);
        assert_eq!(check("Water", &[row(rho * (1.0 + 1e-6), 0.0)]).failures.len(), 1, "ρ 1e-6 off");
        assert_eq!(check("Water", &[row(rho, 1e-6 * state.h(b))]).failures.len(), 0, "h exempt");
    }

    /// `beyond`, exactly: at the bound is not beyond it, past it is.
    #[test]
    fn beyond_its_bound_is_strict() {
        assert!(!beyond(0.0, 1.0, 1.0) && beyond(0.0, 2.0, 1.0) && beyond(2.0, 0.0, 1.0) && !beyond(1.0, 1.0, 0.0));
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
        let check = FlashCheck { compared: 4, failures: vec!["x".into()], ..Default::default() };
        assert_eq!(check.report(1), Some("1 of 4 entries outside Flash (headroom 0.000):\nx".into()));
        assert_eq!(FlashCheck::default().report(1), None);
    }
}
