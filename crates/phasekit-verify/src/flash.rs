//! Checking `flash` fixtures (VERIFICATION.md §3.5, §8.2), shared by the corpus tests and the nightly sweep: a pair's
//! inputs read off a truth state, flashed with no phase imposed, against CoolProp's flash of the same inputs. The DT rows
//! since M5.3, (ρ, T); the PT rows since M7.1, the truth's own (p, T), whose ρ is an output too. The phase label and Q
//! (class `Flash`, 1e-8 absolute) must match, and ρ (PT), p, h, s, u within class `Flash` (1e-9 of max(|v|, floor)). A
//! single-phase state is the EOS relation at its (T, ρ), so the `Term` bound carried through that relation applies to
//! it too (user decision TC1).

use phasekit_core::internal::FluidRecord;
use phasekit_core::{
    Basis, Density, DerivVar, Enthalpy, Entropy, Error, Fluid, Input, InternalEnergy, Order, Partial, Phase, Pressure,
    Quality, State, Temperature,
};

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

    /// PLAN.md M7.3 (VERIFICATION.md §8.2): the P+X round trips of every truth in a one-fluid `flash` file. A PT truth
    /// is the PT state at its (p, T), a QT truth the QT state at its (T, Q), and a pseudo-pure fluid's two-phase `dome`
    /// truth the PQ state at its (p, Q): inside its dome CoolProp's PH, like its PQ, follows the ancillaries, T linear in
    /// Q between the bubble and dew temperatures. A single-phase `dome` truth is left out: two of Air's are gas by
    /// CoolProp's density bands (M6.9) yet lie inside its ancillary dome at their own p, where CoolProp's PH, and
    /// phasekit's, is two-phase (Air at 68.85 K and 18.2 kPa: PH gives 69.599 K at Q 0.99641, both alike to 3e-14). PH, PS and PU of each truth's p and h, s or u give its phase, its T and
    /// ρ within class `Flash` (1e-9 of each) and, in the dome, its quality within 1e-8.
    pub fn px_round_trips(&mut self, fixture: &Fixture<'_>, fluid: &Fluid) {
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap_or_default();
        let label = |row: usize, column: &str| fixture.printed(row, column).unwrap_or_default();
        for row in 0..fixture.rows().len() {
            let number = |column: &str| fixture.value(row, column).unwrap_or(f64::NAN);
            let (temperature, pressure) = (Temperature::new(number("x2")), Pressure::new(number("p")));
            let truth = match (label(row, "pair"), label(row, "truth")) {
                ("PT", "PT") => Pressure::new(number("x1")).and_then(|p| Ok(Input::pt(p, temperature?))),
                ("DT", "QT") => Quality::new(number("Q")).and_then(|q| Ok(Input::qt(q, temperature?))),
                ("DT", "dome") if label(row, "phase") == "twophase" => {
                    Quality::new(number("Q")).and_then(|q| Ok(Input::pq(pressure?, q)))
                }
                _ => continue,
            };
            match truth.and_then(|input| fluid.state(input)) {
                Ok(truth) => self.round_trip(name, fluid, &truth, (p_x(&truth), (0.0, 0.0))),
                Err(e) => self.failures.push(format!("{name} row {row}: the truth fails: {e:?}")),
            }
        }
    }

    /// PLAN.md M7.4 (VERIFICATION.md §8.2): the D+X round trips of every truth in a one-fluid `flash` file, as
    /// [`Self::px_round_trips`] but every `dome` truth its DT state: DH, DS and DU invert DT along the isochore, so a
    /// state DT labels is its own truth.
    pub fn dx_round_trips(&mut self, fixture: &Fixture<'_>, fluid: &Fluid) {
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap_or_default();
        let label = |row: usize, column: &str| fixture.printed(row, column).unwrap_or_default();
        for row in 0..fixture.rows().len() {
            let number = |column: &str| fixture.value(row, column).unwrap_or(f64::NAN);
            let temperature = Temperature::new(number("x2"));
            let truth = match (label(row, "pair"), label(row, "truth")) {
                ("PT", "PT") => Pressure::new(number("x1")).and_then(|p| Ok(Input::pt(p, temperature?))),
                ("DT", "QT") => Quality::new(number("Q")).and_then(|q| Ok(Input::qt(q, temperature?))),
                ("DT", "dome") => Density::molar(number("x1")).and_then(|d| Ok(Input::dt(d, temperature?))),
                _ => continue,
            };
            match truth.and_then(|input| fluid.state(input)) {
                Ok(truth) => self.dx_round_trip(name, fluid, &truth),
                Err(e) => self.failures.push(format!("{name} row {row}: the truth fails: {e:?}")),
            }
        }
    }

    /// DH, DS and DU of one truth state give its phase, T and ρ within class `Flash` and, in the dome, its quality
    /// within 1e-8.
    pub fn dx_round_trip(&mut self, name: &str, fluid: &Fluid, truth: &State) {
        self.round_trip(name, fluid, truth, (d_x(truth), (0.0, 0.0)));
    }

    /// PLAN.md M7.8: SU of every PT truth in a one-fluid `flash` file (map 01 R4: CoolProp has no SU, so its truth comes
    /// from (p, T) only).
    pub fn su_round_trips(&mut self, fixture: &Fixture<'_>, fluid: &Fluid) {
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap_or_default();
        for row in 0..fixture.rows().len() {
            if (fixture.printed(row, "pair"), fixture.printed(row, "truth")) != (Some("PT"), Some("PT")) {
                continue;
            }
            let number = |column: &str| fixture.value(row, column).unwrap_or(f64::NAN);
            let truth = Pressure::new(number("x1")).and_then(|p| Ok(Input::pt(p, Temperature::new(number("x2"))?)));
            match truth.and_then(|input| fluid.state(input)) {
                Ok(truth) => self.su_round_trip(name, fluid, &truth),
                Err(e) => self.failures.push(format!("{name} row {row}: the truth fails: {e:?}")),
            }
        }
    }

    /// HS of one truth state gives its phase, T and ρ within class `Flash` and, in the dome, its quality within 1e-8.
    pub fn hs_round_trip(&mut self, name: &str, fluid: &Fluid, truth: &State) {
        let m = Basis::Molar;
        let input = Enthalpy::molar(truth.h(m)).and_then(|h| Ok([Input::hs(h, Entropy::molar(truth.s(m))?)]));
        self.round_trip(name, fluid, truth, (input, (0.0, 0.0)));
    }

    /// SU of one truth state, as [`Self::hs_round_trip`], but T and ρ within the conditioning of (s, u) where that is
    /// wider. Along an isentrope u moves with p by (∂u/∂p)_s = (p/ρ²)·(∂ρ/∂p)_s, little in a liquid at low p, so SU's
    /// resolution in u, 1e-13 of max(|u|, RT) (`flash::ISENTROPE_NOISE`), leaves p uncertain by that over (∂u/∂p)_s,
    /// and T and ρ by their own slopes along the isentrope times that; twice that bounds them (Methanol's liquid at
    /// 168 Pa: p within 230 Pa, T within 2e-5 K).
    pub fn su_round_trip(&mut self, name: &str, fluid: &Fluid, truth: &State) {
        let m = Basis::Molar;
        let input = Entropy::molar(truth.s(m)).and_then(|s| Ok([Input::su(s, InternalEnergy::molar(truth.u(m))?)]));
        self.round_trip(name, fluid, truth, (input, su_floors(truth)));
    }

    /// The round trips of one truth state through `inputs`, read off it.
    fn round_trip<const N: usize>(
        &mut self,
        name: &str,
        fluid: &Fluid,
        truth: &State,
        (inputs, floors): (Result<[Input; N], Error>, (f64, f64)),
    ) {
        let Ok(inputs) = inputs else {
            self.failures.push(format!("{name}: no inputs from {truth:?}"));
            return;
        };
        for input in inputs {
            self.compared += 1;
            self.two_phase += usize::from(truth.phase() == Phase::TwoPhase);
            let back = match fluid.state(input) {
                Ok(back) => back,
                Err(e) => {
                    self.failures.push(format!("{name} {input:?} of {truth:?}: {e:?}"));
                    continue;
                }
            };
            let window = fluid
                .info()
                .published_critical()
                .map_or(Window::Regular, |c| Window::at(truth.t(), truth.rho(Basis::Molar), c.t, c.rho));
            if !agrees(coordinates(&back), coordinates(truth), window, floors) {
                self.failures.push(format!("{name} {input:?}: {back:?} against {truth:?}"));
            }
        }
    }

    /// `None` when every row agreed, else the first `shown` disagreements.
    pub fn report(&self, shown: usize) -> Option<String> {
        report(&self.failures, self.compared, 0.0, "Flash", shown)
    }
}

/// A state's phase, T, molar ρ and quality, which a round trip compares.
type Coordinates = (Phase, f64, f64, Option<f64>);

/// The coordinates of `state`.
fn coordinates(state: &State) -> Coordinates {
    (state.phase(), state.t(), state.rho(Basis::Molar), state.quality())
}

/// The floors of T and ρ for an SU round trip of `truth` ([`FlashCheck::su_round_trip`]): SU's resolution in u, twice
/// 1e-13 of max(|u|, RT), over (∂u/∂p)_s, times T's and ρ's own slopes along the isentrope.
pub(crate) fn su_floors(truth: &State) -> (f64, f64) {
    let along = |of| truth.partial(Partial { of, wrt: DerivVar::P, at: DerivVar::Smolar }).map_or(0.0, f64::abs);
    let resolution = 2e-13 * truth.u(Basis::Molar).abs().max(truth.gas_constant() * truth.t());
    let dp = resolution / along(DerivVar::Umolar);
    (along(DerivVar::T) * dp, along(DerivVar::Dmolar) * dp)
}

/// Class `Flash`'s bounds for a solved input in the near-critical window (VERIFICATION.md §5): T 1e-8, ρ 1e-6 of each,
/// and Q 1e-6 absolute (measured at M7.8: D+X of n-Propane's and Methane's dome at their published Tc, where ρ′ and ρ″
/// differ by 0.2 %, returns Q within 2.5e-8).
const NEAR_CRITICAL: (f64, f64, f64) = (1e-8, 1e-6, 1e-6);

/// Whether a round trip's state agrees with its truth (class `Flash`): the same phase, T and ρ within 1e-9 of the
/// truth's, and its quality within 1e-8, or no quality on both; in the near-critical `window`, [`NEAR_CRITICAL`]'s
/// bounds; and T and ρ within floors where the pair leaves them less determined (SU, [`FlashCheck::su_round_trip`]). A
/// dome's end is the single phase it borders: Q = 0 within its bound the liquid, Q = 1 the gas (a D+X round trip of a
/// saturated state lands on the dome's edge, either side).
fn agrees(back: Coordinates, truth: Coordinates, window: Window, (t_floor, rho_floor): (f64, f64)) -> bool {
    let near = window == Window::NearCritical;
    let (t_tol, rho_tol, q_tol) = if near { NEAR_CRITICAL } else { (1e-9, 1e-9, ToleranceClass::FLASH_QUALITY) };
    let off = |got: f64, want: f64, tol: f64, floor: f64| beyond(got, want, (tol * want.abs()).max(floor));
    let edge = |a: Coordinates, b: Coordinates| match (a.3, b.0) {
        (Some(q), Phase::Liquid) => !beyond(q, 0.0, q_tol),
        (Some(q), Phase::Gas) => !beyond(q, 1.0, q_tol),
        _ => false,
    };
    let quality = match (back.3, truth.3) {
        (Some(got), Some(want)) => !beyond(got, want, q_tol),
        (None, None) => true,
        _ => edge(back, truth) || edge(truth, back),
    };
    let phase = back.0 == truth.0 || edge(back, truth) || edge(truth, back);
    phase && quality && !off(back.1, truth.1, t_tol, t_floor) && !off(back.2, truth.2, rho_tol, rho_floor)
}

/// The P+X inputs of a state, molar: (p, h), (p, s) and (p, u).
fn p_x(state: &State) -> Result<[Input; 3], Error> {
    let (p, m) = (Pressure::new(state.p())?, Basis::Molar);
    Ok([
        Input::hp(Enthalpy::molar(state.h(m))?, p),
        Input::ps(p, Entropy::molar(state.s(m))?),
        Input::pu(p, InternalEnergy::molar(state.u(m))?),
    ])
}

/// The D+X inputs of a state, molar: (ρ, h), (ρ, s) and (ρ, u).
fn d_x(state: &State) -> Result<[Input; 3], Error> {
    let m = Basis::Molar;
    let d = Density::molar(state.rho(m))?;
    Ok([
        Input::dh(d, Enthalpy::molar(state.h(m))?),
        Input::ds(d, Entropy::molar(state.s(m))?),
        Input::du(d, InternalEnergy::molar(state.u(m))?),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use phasekit_core::{DataSet, Registry, State};

    /// A round trip agrees with its truth only where its phase, T, ρ and quality all do, each within class `Flash`; in
    /// the near-critical window within its wider bounds, and ρ within a pair's own floor.
    #[test]
    fn round_trips_compare_every_coordinate() {
        let regular = |back, truth| agrees(back, truth, Window::Regular, (0.0, 0.0));
        let truth = (Phase::Liquid, 300.0, 5e4, None);
        assert!(regular(truth, truth) && regular((Phase::Liquid, 300.0 * (1.0 + 5e-10), 5e4, None), truth));
        let apart = [
            (Phase::Gas, 300.0, 5e4, None),
            (Phase::Liquid, 300.0 * (1.0 + 2e-9), 5e4, None),
            (Phase::Liquid, 300.0, 5e4 * (1.0 + 2e-9), None),
            (Phase::Liquid, 300.0, 5e4, Some(0.5)),
        ];
        for back in apart {
            assert!(!regular(back, truth), "{back:?}");
        }
        let wet = (Phase::TwoPhase, 300.0, 5e4, Some(0.5));
        assert!(regular((Phase::TwoPhase, 300.0, 5e4, Some(0.5 + 5e-9)), wet));
        assert!(!regular((Phase::TwoPhase, 300.0, 5e4, Some(0.5 + 2e-8)), wet));
        // A dome's end is the single phase it borders, either way round, and no other.
        let (bubble, dew) =
            ((Phase::TwoPhase, 300.0, 5e4, Some(5e-9)), (Phase::TwoPhase, 300.0, 5e4, Some(1.0 - 5e-9)));
        assert!(regular(bubble, truth) && regular(truth, bubble) && regular(dew, (Phase::Gas, 300.0, 5e4, None)));
        assert!(!regular(dew, truth) && !regular((Phase::TwoPhase, 300.0, 5e4, Some(2e-8)), truth));
        assert!(!regular(bubble, (Phase::Gas, 300.0, 5e4, None)));
        assert!(!regular(bubble, (Phase::Supercritical, 300.0, 5e4, None)));
        // Near the critical point T within 1e-8, ρ within 1e-6 and Q within 1e-6; and ρ within a pair's floor.
        let near = |back| agrees(back, wet, Window::NearCritical, (0.0, 0.0));
        assert!(near((Phase::TwoPhase, 300.0 * (1.0 + 5e-9), 5e4 * (1.0 + 5e-7), Some(0.5 + 5e-7))));
        assert!(!near((Phase::TwoPhase, 300.0 * (1.0 + 2e-8), 5e4, Some(0.5))));
        assert!(!near((Phase::TwoPhase, 300.0, 5e4 * (1.0 + 2e-6), Some(0.5))));
        assert!(!near((Phase::TwoPhase, 300.0, 5e4, Some(0.5 + 2e-6))));
        let floored = |back, floor| agrees(back, truth, Window::Regular, floor);
        assert!(floored((Phase::Liquid, 300.0, 5e4 + 1e-3, None), (0.0, 2e-3)));
        assert!(!floored((Phase::Liquid, 300.0, 5e4 + 1e-3, None), (0.0, 5e-4)));
        assert!(floored((Phase::Liquid, 300.0 + 1e-5, 5e4, None), (2e-5, 0.0)));
        assert!(!floored((Phase::Liquid, 300.0 + 1e-5, 5e4, None), (5e-6, 0.0)));
    }

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

    /// SU's floors are its conditioning and no looser: inside class `Flash` for Water's gas at 1 bar and 400 K (under
    /// 1e-10 of T and ρ), and of the order its slow u along the isentrope leaves for Methanol's liquid at 168 Pa and
    /// 200 K (T between 1e-6 and 1e-3 K). By Water's triple point, where its liquid's u is about 0, SU resolves u to
    /// 1e-13 of RT, not of |u|: T within 1e-10 to 1e-9 K, ρ within 1e-6 to 1e-5 mol/m³.
    #[test]
    fn su_floors_are_su_conditioning() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let state = |name: &str, p: f64, t: f64| {
            let (p, t) = (phasekit_core::Pressure::new(p).unwrap(), phasekit_core::Temperature::new(t).unwrap());
            registry.get(name).unwrap().state(Input::pt(p, t)).unwrap()
        };
        let gas = state("Water", 1e5, 400.0);
        let (t_floor, rho_floor) = su_floors(&gas);
        assert!(t_floor < 1e-10 * gas.t() && rho_floor < 1e-10 * gas.rho(Basis::Molar), "{t_floor} {rho_floor}");
        let liquid = state("Methanol", 168.0, 200.0);
        let (t_floor, rho_floor) = su_floors(&liquid);
        assert!(liquid.phase() == Phase::Liquid && t_floor > 1e-6 && t_floor < 1e-3, "{t_floor} {liquid:?}");
        assert!(rho_floor > 0.0 && rho_floor < 1e-3 * liquid.rho(Basis::Molar), "{rho_floor}");
        let triple = state("Water", 1e6, 273.16);
        let (t_floor, rho_floor) = su_floors(&triple);
        assert!(triple.u(Basis::Molar).abs() < 1.0 && t_floor > 1e-10 && t_floor < 1e-9, "{t_floor} {triple:?}");
        assert!(rho_floor > 1e-6 && rho_floor < 1e-5, "{rho_floor}");
    }
}
