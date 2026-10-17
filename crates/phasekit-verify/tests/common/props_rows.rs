//! The `props` kind's rows (PLAN.md M10.2; VERIFICATION.md §3.5), checked through the compat strings: CoolProp's 85
//! outputs at DT and QT states and its trivial outputs through `Props1SI`. Shared by the core subset's test
//! (`tests/props.rs`) and the nightly sweep of the full set (`tests/sweeps.rs`). A test binary includes it with
//! `#[path = "common/props_rows.rs"] mod props_rows;` beside `common/sweep.rs`.

use std::collections::BTreeMap;

use phasekit_compat::{CompatError, parse_derivative, phase_si_in, props_si_in, props1_si_in};
use phasekit_core::internal::{ConductivityModel, FluidRecord, ViscosityModel};
use phasekit_core::{Constant, DerivVar, Order, Prop, PureFluid, Registry};
use phasekit_verify::eos::{
    Majorants, carried, carried_partial, carried_second_partial, partial_floor, second_partial_floor,
};
use phasekit_verify::fixture::Kind;
use phasekit_verify::majorant;
use phasekit_verify::register::{RowKey, exempt_row};
use phasekit_verify::term::IdealScale;
use phasekit_verify::tolerance::Tolerance;
use phasekit_verify::{DIVERGENCES, Fixture, ToleranceClass, Window};

use crate::sweep::Failures;

/// Energies, whose `Prop` floor is RT (VERIFICATION.md §5), molar then mass.
const ENERGIES: [&str; 14] = [
    "Hmolar",
    "Umolar",
    "Gmolar",
    "Helmholtzmolar",
    "Hmolar_residual",
    "Gmolar_residual",
    "Hmolar_idealgas",
    "Umolar_idealgas",
    "Hmass",
    "Umass",
    "Gmass",
    "Helmholtzmass",
    "Hmass_idealgas",
    "Umass_idealgas",
];

/// Entropies and heat capacities, whose `Prop` floor is R, molar then mass.
const ENTROPIES: [&str; 11] = [
    "Smolar",
    "Smolar_residual",
    "Smolar_idealgas",
    "Cpmolar",
    "Cvmolar",
    "Cp0molar",
    "Smass",
    "Smass_idealgas",
    "Cpmass",
    "Cvmass",
    "Cp0mass",
];

/// The outputs a state inside the dome takes from the split of its saturation, a `Flash` result: its p, density, the
/// caloric properties of the two phases weighed by Q, Q itself and the reduced density.
const FLASHED: [&str; 16] = [
    "P",
    "Dmolar",
    "Dmass",
    "Hmolar",
    "Hmass",
    "Smolar",
    "Smass",
    "Umolar",
    "Umass",
    "Gmolar",
    "Gmass",
    "Helmholtzmolar",
    "Helmholtzmass",
    "Q",
    "Qmass",
    "Delta",
];

/// One state of a file: its pair and inputs, whether it lies inside the dome, and the (T, ρ) of its one phase (single or
/// saturated) and its p where the kernel gives them.
struct At<'a> {
    pair: &'a str,
    x: [f64; 2],
    two_phase: bool,
    one: Option<(f64, f64)>,
    p: f64,
}

impl<'a> At<'a> {
    /// The state of a row of `name`.
    fn new(registry: &Registry, name: &str, pair: &'a str, x: [f64; 2]) -> Self {
        let call = |output| match pair {
            "DT" => props_si_in(registry, output, "T", x[1], "Dmolar", x[0], name),
            "QT" => props_si_in(registry, output, "Q", x[0], "T", x[1], name),
            _ => Err(CompatError::NotAConstant(output.into())),
        };
        let two_phase = match pair {
            "QT" => 0.0 < x[0] && x[0] < 1.0,
            "DT" => phase_si_in(registry, "T", x[1], "Dmolar", x[0], name) == Ok("twophase"),
            _ => false,
        };
        let one = if two_phase { None } else { call("Dmolar").ok().map(|rho| (x[1], rho)) };
        At { pair, x, two_phase, one, p: call("P").unwrap_or(f64::NAN) }
    }

    /// A first-order variable's value at this state, through the strings (its key is its name).
    fn value(&self, registry: &Registry, name: &str, v: DerivVar) -> f64 {
        let key = format!("{v:?}");
        let got = match self.pair {
            "DT" => props_si_in(registry, &key, "T", self.x[1], "Dmolar", self.x[0], name),
            _ => props_si_in(registry, &key, "Q", self.x[0], "T", self.x[1], name),
        };
        got.unwrap_or(f64::NAN)
    }
}

/// The register's name of a `props` column: a derivative string's pattern, `d(X)/d(Y)|Z` or `d(d(X)/d(Y)|Z)/d(W)|V`, so
/// an entry exempts every one of its strings; any other output its own name.
fn register_column(output: &str) -> &str {
    match parse_derivative(output) {
        Some(Prop::Partial(_)) => "d(X)/d(Y)|Z",
        Some(Prop::SecondPartial(_)) => "d(d(X)/d(Y)|Z)/d(W)|V",
        _ => output,
    }
}

/// A fluid's model data, for the bounds: its record, compiled package and ideal-gas scale, gas constant, molar mass and
/// published critical point, and whether its η and λ are ECS (class `TransportEcs`).
struct Model {
    record: FluidRecord,
    compiled: PureFluid,
    ideal: IdealScale,
    r: f64,
    m: f64,
    critical: Option<(f64, f64)>,
    ecs: [bool; 2],
}

impl Model {
    fn new(registry: &Registry, name: &str) -> Self {
        let fluid = registry.get(name).unwrap();
        let record = phasekit_core::internal::record(registry, name).unwrap();
        let ideal = IdealScale::new(&record).unwrap();
        let (r, m) = (fluid.constant(Constant::GasConstant).unwrap(), fluid.constant(Constant::MolarMass).unwrap());
        let transport = record.transport.as_ref();
        let eta =
            transport.and_then(|t| t.viscosity.as_ref()).is_some_and(|v| matches!(v.model, ViscosityModel::Ecs(_)));
        let lambda = transport
            .and_then(|t| t.conductivity.as_ref())
            .is_some_and(|c| matches!(c.model, ConductivityModel::Ecs(_)));
        let critical = fluid.info().published_critical().map(|c| (c.t, c.rho));
        let compiled = record.clone().compile().unwrap();
        Model { record, compiled, ideal, r, m, critical, ecs: [eta, lambda] }
    }

    /// Whether `output` reads an ECS model.
    fn ecs(&self, output: &str) -> bool {
        match output {
            "viscosity" => self.ecs[0],
            "conductivity" => self.ecs[1],
            "Prandtl" => self.ecs[0] || self.ecs[1],
            _ => false,
        }
    }

    /// The bound of `output`'s relation at one phase's (T, ρ) (VERIFICATION.md §5), for a cell its own bound does not
    /// hold: `Term`'s, carried through the relation by `phasekit_verify::eos::carried` (g and a carry h's or u's and T
    /// times s's; κ_s = ρw²/p carries 2δw/w + δp/p of itself; mass outputs over M; an α output its entry's scale over
    /// τ^i δ^j; a first partial through its Jacobian ratio, a second to order 3; the PIP, a sum of ratios of A_ij to
    /// order 3, Σ|∂PIP/∂A_ij|·M_ij, which grows without bound where (∂p/∂T)_ρ vanishes, at Water's density maximum).
    /// 0 for an output without a relation there.
    fn relation_bound(&self, output: &str, (t, rho): (f64, f64)) -> f64 {
        let eos = self.compiled.eos();
        let total = eos.ideal(t, rho, Order::Three) + eos.residual(t, rho, Order::Three);
        let (Some(bundle), Some(a)) =
            (total.bundle(), total.get(0, 0).map(|_| |i, j| total.get(i, j).unwrap_or(f64::NAN)))
        else {
            return 0.0;
        };
        if let Some((residual, i, j)) = alpha_entry(output) {
            // An α output is A_ij/(τ^i δ^j); its entry's `Term` scale, scaled the same (the `term` kind's class).
            let e = &self.record.eos;
            let (tau, delta) = (e.t_reducing / t, rho / e.rho_reducing);
            let scale = if residual { majorant::eos(e, tau, delta, i, j) } else { self.ideal.get(t, rho, i, j) };
            let per = phasekit_core::math::powi(tau, i as i32) * phasekit_core::math::powi(delta, j as i32);
            return ToleranceClass::Term.bound(scale / per).unwrap_or(0.0);
        }
        if output == "PIP" {
            // PIP = 2 − N1/D1 + N2/D2, N1 = 2A01 + A02 − 2A11 − A12, D1 = A01 − A11, N2 = 2A01 + 4A02 + A03, D2 = 2A01 + A02.
            let (n1, d1) = (2.0 * a(0, 1) + a(0, 2) - 2.0 * a(1, 1) - a(1, 2), a(0, 1) - a(1, 1));
            let (n2, d2) = (2.0 * a(0, 1) + 4.0 * a(0, 2) + a(0, 3), 2.0 * a(0, 1) + a(0, 2));
            let (tau, delta) = (self.record.eos.t_reducing / t, rho / self.record.eos.rho_reducing);
            let m = |i, j| majorant::eos(&self.record.eos, tau, delta, i, j) + self.ideal.get(t, rho, i, j);
            let sensitivities = [
                ((0, 1), -(2.0 / d1 - n1 / (d1 * d1)) + (2.0 / d2 - 2.0 * n2 / (d2 * d2))),
                ((0, 2), -1.0 / d1 + (4.0 / d2 - n2 / (d2 * d2))),
                ((0, 3), 1.0 / d2),
                ((1, 1), 2.0 / d1 - n1 / (d1 * d1)),
                ((1, 2), 1.0 / d1),
            ];
            let carried: f64 = sensitivities.iter().map(|&((i, j), ds)| ds.abs() * m(i, j)).sum();
            return ToleranceClass::Term.bound(carried).unwrap_or(0.0);
        }
        let majorants = Majorants::at(&self.record, &self.ideal, t, rho);
        let spread = match parse_derivative(output) {
            Some(Prop::Partial(p)) => Some(carried_partial(p, &bundle, &majorants, self.r, t, rho, self.m)),
            Some(Prop::SecondPartial(p)) => {
                Some(carried_second_partial(p, eos, (&self.record, &self.ideal), [self.r, t, rho, self.m]))
            }
            _ => None,
        };
        if let Some(spread) = spread {
            return ToleranceClass::Term.bound(spread).unwrap_or(0.0);
        }
        let column = |c| carried(c, &bundle, &majorants, self.r, t, rho, self.m);
        let molar = match output.replace("mass", "molar").as_str() {
            "P" => column("p"),
            "Hmolar" | "Hmolar_residual" | "Hmolar_idealgas" => column("hmolar"),
            "Smolar" | "Smolar_residual" | "Smolar_idealgas" => column("smolar"),
            "Umolar" | "Umolar_idealgas" => column("umolar"),
            "Gmolar" | "Gmolar_residual" => column("hmolar") + t * column("smolar"),
            "Helmholtzmolar" => column("umolar") + t * column("smolar"),
            "Cvmolar" => column("cvmolar"),
            "Cpmolar" => column("cpmolar"),
            "speed_of_sound" => return ToleranceClass::Term.bound(column("speed_sound")).unwrap_or(0.0),
            "Z" => column("Z"),
            "isentropic_expansion_coefficient" => {
                let (x, d) = (bundle.a01 - bundle.a11, 2.0 * bundle.a01 + bundle.a02);
                let (w2, p) = (self.r * t / self.m * (d - x * x / bundle.a20), rho * self.r * t * bundle.a01);
                let kappa_s = rho * self.m * w2 / p;
                return ToleranceClass::Term
                    .bound(
                        (kappa_s
                            * (column("speed_sound") / phasekit_core::math::sqrt(w2) * 2.0 + column("p") / p.abs()))
                        .abs(),
                    )
                    .unwrap_or(0.0);
            }
            _ => f64::NAN,
        };
        let carried = if output.contains("mass") { molar / self.m } else { molar };
        ToleranceClass::Term.bound(carried).unwrap_or(0.0)
    }
}

/// The α output an output key names, as (residual part, i, j) of its A_ij.
fn alpha_entry(output: &str) -> Option<(bool, usize, usize)> {
    Some(match output {
        "alphar" => (true, 0, 0),
        "dalphar_dtau_constdelta" => (true, 1, 0),
        "dalphar_ddelta_consttau" => (true, 0, 1),
        "alpha0" => (false, 0, 0),
        "dalpha0_dtau_constdelta" => (false, 1, 0),
        "dalpha0_ddelta_consttau" => (false, 0, 1),
        "d2alpha0_ddelta2_consttau" => (false, 0, 2),
        "d3alpha0_ddelta3_consttau" => (false, 0, 3),
        _ => return None,
    })
}

/// `Prop`'s floor of `output` at temperature `t` and pressure `p` (VERIFICATION.md §5, user decision OD2): RT for an
/// energy, R for an entropy or heat capacity (over M in mass units), 1/|p| for κ_T, 1/T for β, 1 for the dimensionless
/// derivative outputs; 0 otherwise.
fn floor(output: &str, (r, m): (f64, f64), t: f64, p: f64) -> f64 {
    let per = if output.contains("mass") { 1.0 / m } else { 1.0 };
    match output {
        _ if ENERGIES.contains(&output) => r * t * per,
        _ if ENTROPIES.contains(&output) => r * per,
        "isothermal_compressibility" => 1.0 / p.abs(),
        "isobaric_expansion_coefficient" => 1.0 / t,
        "PIP" | "fundamental_derivative_of_gas_dynamics" | "isentropic_expansion_coefficient" => 1.0,
        _ => 0.0,
    }
}

/// `Prop`'s floor of a derivative string (VERIFICATION.md §5, user decision OD2): |X|/|Y| of a first partial,
/// |X|/(|Y|·|W|) of a second, from the variables' values at the state (`value`); 0 for any other output.
fn derivative_floor(output: &str, value: impl Fn(DerivVar) -> f64) -> f64 {
    match parse_derivative(output) {
        Some(Prop::Partial(p)) => partial_floor(value(p.of), value(p.wrt)),
        Some(Prop::SecondPartial(p)) => second_partial_floor(value(p.first.of), value(p.first.wrt), value(p.wrt)),
        _ => 0.0,
    }
}

/// The bound of `output` near `want` at the state `at` (VERIFICATION.md §5): `Prop`'s own at max(|want|, floor), or,
/// inside the dome, `Flash`'s on what the saturation split gives (Q's absolute); the numerical critical point within
/// `Flash`'s near-critical bounds, as the `crit` kind's; η and λ in their model's class, as the `transport` kind's; B
/// and dB/dT within DIV-0011's measured bound, a saturated phase's Z within DIV-0044's; the phase index exactly.
fn bound(output: &str, (want, derivative_floor): (f64, f64), model: &Model, at: &At<'_>, window: Window) -> f64 {
    let t = at.x[1];
    let scale = want.abs().max(floor(output, (model.r, model.m), t, at.p)).max(derivative_floor);
    let measured = |id: &str| match DIVERGENCES.iter().find(|d| d.id == id).and_then(|d| d.tolerance) {
        Some(Tolerance::Relative(x)) => x * scale,
        _ => 0.0,
    };
    let saturated = at.pair == "QT" && !at.two_phase;
    let transport = if model.ecs(output) { ToleranceClass::TransportEcs } else { ToleranceClass::TransportDirect };
    match output {
        "Phase" => 0.0,
        "T_critical" => ToleranceClass::FLASH_NC_T * scale,
        "rhomolar_critical" | "rhomass_critical" => ToleranceClass::FLASH_NC_RHO * scale,
        "p_critical" => ToleranceClass::Flash.bound(scale).unwrap(),
        "Bvirial" | "dBvirial_dT" => measured("DIV-0011"),
        "Z" if saturated => measured("DIV-0044"),
        "viscosity" | "conductivity" | "Prandtl" => transport.bound(scale).unwrap(),
        "Q" | "Qmass" if at.two_phase => ToleranceClass::FLASH_QUALITY,
        _ if at.two_phase && FLASHED.contains(&output) => ToleranceClass::Flash.bound(scale).unwrap(),
        _ => ToleranceClass::Prop.bound_in(scale, window).unwrap(),
    }
}

/// What the rows of `props` files gave: per output, the rows compared within their bound and the rows both sides
/// refuse; per register entry, the cells it exempts; the ECS cells phasekit's conformal solve gives
/// where the oracle's fails (as the `transport` kind counts them); every disagreement; the largest fraction of a bound
/// used.
#[derive(Default)]
pub struct PropsCheck {
    pub compared: BTreeMap<String, usize>,
    pub refused: BTreeMap<String, usize>,
    pub exempt: BTreeMap<&'static str, usize>,
    pub ecs_beyond_oracle: usize,
    pub failures: Failures,
    pub headroom: f64,
}

impl PropsCheck {
    /// Every row of `table`, the `props` file of `name`, through `registry` (`Parity`).
    pub fn file(&mut self, registry: &Registry, name: &str, table: &Fixture<'_>) {
        let model = Model::new(registry, name);
        let mut at: Option<At<'_>> = None;
        for row in 0..table.rows().len() {
            let pair = table.printed(row, "pair").unwrap();
            let x = [table.value(row, "x1").unwrap(), table.value(row, "x2").unwrap()];
            if at.as_ref().is_none_or(|a| a.pair != pair || a.x.map(f64::to_bits) != x.map(f64::to_bits)) {
                at = Some(At::new(registry, name, pair, x));
            }
            if let Some(state) = &at {
                self.row(registry, (name, table, row), state, &model);
            }
        }
    }

    /// One row: exempt, refused by both, compared, or a disagreement.
    fn row(
        &mut self,
        registry: &Registry,
        (name, table, row): (&str, &Fixture<'_>, usize),
        at: &At<'_>,
        model: &Model,
    ) {
        let output = table.printed(row, "output").unwrap();
        let [x1, x2] = at.x;
        let key = RowKey {
            t: x2,
            input: Some(at.pair),
            two_phase: at.two_phase,
            p: None,
            balanced: true,
            near_critical: false,
        };
        if let Some(id) = exempt_row(DIVERGENCES, name, Kind::Props, register_column(output), key) {
            *self.exempt.entry(id).or_default() += 1;
            return;
        }
        let got = match at.pair {
            "Props1SI" => props1_si_in(registry, output, name),
            "DT" => props_si_in(registry, output, "T", x2, "Dmolar", x1, name),
            _ => props_si_in(registry, output, "Q", x1, "T", x2, name),
        };
        let place = format!("{name} {} {x1} {x2} {output}", at.pair);
        match (got, table.printed(row, "status") == Some("ok")) {
            (Ok(value), true) => {
                let floor = derivative_floor(output, |v| at.value(registry, name, v));
                self.compare((table, row), (output, floor), value, (at, model), &place);
            }
            (Err(_), false) => *self.refused.entry(output.into()).or_default() += 1,
            (Ok(_), false) if model.ecs(output) => self.ecs_beyond_oracle += 1,
            (Ok(value), false) => self.failures.check(false, || format!("{place}: {value} here, the oracle refuses")),
            (Err(e), true) => {
                let want = table.value(row, "value").unwrap();
                self.failures.check(false, || format!("{place}: {e:?} here, the oracle's {want}"));
            }
        }
    }

    /// A cell both sides give, within [`bound`], or within its relation's ([`Model::relation_bound`], computed only where
    /// the own bound does not hold: it costs an order-3 evaluation and the majorants).
    fn compare(
        &mut self,
        (table, row): (&Fixture<'_>, usize),
        (output, derivative_floor): (&str, f64),
        value: f64,
        (at, model): (&At<'_>, &Model),
        place: &str,
    ) {
        let want = table.value(row, "value").unwrap();
        let window = match (at.pair, model.critical) {
            ("DT", Some((tc, rhoc))) => Window::at(at.x[1], at.x[0], tc, rhoc),
            _ => Window::Regular,
        };
        let mut bound = bound(output, (want, derivative_floor), model, at, window);
        if (value - want).abs() > bound && !at.two_phase {
            bound = bound.max(at.one.map_or(0.0, |one| model.relation_bound(output, one)));
        }
        if let Some(ratio) =
            self.failures.ok(table.check_bound(row, "value", value, bound), |e| format!("{place}: {e:?}"))
        {
            self.headroom = self.headroom.max(ratio);
            *self.compared.entry(output.into()).or_default() += 1;
        }
    }
}
