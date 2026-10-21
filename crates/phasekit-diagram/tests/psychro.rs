//! The psychrometric chart (PLAN.md M18.9): every vertex is humid air's W at its T and the line's held quantity, bit
//! for bit (`Exact`); where the saturation line passes a temperature of the authors' Tables 1 and 2, its W is the
//! table's within its printed digits; the families and their runs keep to the range. Mollier's h–x chart (M18.10):
//! every vertex is humid air's at its inputs and drawn at h − r₀W, bit for bit; its 0 °C isotherm lies level, every
//! isotherm meets the saturation line, and that line's W is M18.9's.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Error, Registry};
use phasekit_diagram::{
    ChartLine, ChartRange, Held, HumidAirBatch, Mollier, MollierVertex, Psychrometric, mollier, psychrometric,
};
use phasekit_humidair::{Given, HumidAir, Humidity, Parts, Per};
use phasekit_verify::Fixture;

/// RP-1485's humid air under `Corrected`, over core's water and air and phasekit-iapws's IF97 water and ice.
fn humid_air() -> HumidAir {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let water = registry.get("Water").unwrap().clone();
    let air = registry.get("Air").unwrap().clone();
    let (liquid, ice) =
        (phasekit_iapws::if97::water().unwrap(), phasekit_iapws::ice::ice(&water, DataSet::Corrected).unwrap());
    HumidAir::new(Parts { water, air, liquid, ice }, DataSet::Corrected).unwrap()
}

/// Humid air at one pressure, as the chart samples it.
struct AtP<'a> {
    air: &'a HumidAir,
    p: f64,
}

impl AtP<'_> {
    /// W of the state at `t` holding `held`, where there is one.
    fn ratio(&self, held: Held, t: f64) -> Option<f64> {
        let given = match held {
            Held::Saturation => return self.air.saturated_humidity_ratio(t, self.p).ok().filter(|w| *w >= 0.0),
            Held::Relative(phi) => Given::Humidity(Humidity::Relative(phi)),
            Held::Enthalpy(h) => Given::Enthalpy(h),
            Held::Volume(v) => Given::Volume(v),
            Held::WetBulb(t_wb) => Given::WetBulb(t_wb),
            _ => return None,
        };
        self.air.state(self.p, Given::Temperature(t), given).ok().map(|s| s.humidity_ratio())
    }
}

impl AtP<'_> {
    /// h of the state at (T, W), where there is one.
    fn enthalpy(&self, t: f64, w: f64) -> Option<f64> {
        let state = self.air.state(self.p, Given::Temperature(t), Given::Humidity(Humidity::Ratio(w)));
        state.ok().map(|s| s.h(Per::KgDryAir))
    }

    /// T of the state at `w` holding `held` (h, v or T_wb), where there is one.
    fn temperature(&self, held: Held, w: f64) -> Option<f64> {
        let given = match held {
            Held::Enthalpy(h) => Given::Enthalpy(h),
            Held::Volume(v) => Given::Volume(v),
            Held::WetBulb(t_wb) => Given::WetBulb(t_wb),
            _ => return None,
        };
        self.air.state(self.p, Given::Humidity(Humidity::Ratio(w)), given).ok().map(|s| s.t())
    }
}

impl HumidAirBatch for AtP<'_> {
    fn p(&self) -> f64 {
        self.p
    }

    fn ratios(&self, held: Held, t: &[f64]) -> Result<Vec<Option<f64>>, Error> {
        Ok(t.iter().map(|&t| self.ratio(held, t)).collect())
    }

    fn temperatures(&self, held: Held, w: &[f64]) -> Result<Vec<Option<f64>>, Error> {
        Ok(w.iter().map(|&w| self.temperature(held, w)).collect())
    }

    fn held_at(&self, t: f64, w: f64) -> Result<[f64; 3], Error> {
        let state = self.air.state(self.p, Given::Temperature(t), Given::Humidity(Humidity::Ratio(w)))?;
        Ok([state.h(Per::KgDryAir), state.v(Per::KgDryAir), self.air.wet_bulb(&state)?])
    }

    fn enthalpies(&self, at: &[(f64, f64)]) -> Result<Vec<Option<f64>>, Error> {
        Ok(at.iter().map(|&(t, w)| self.enthalpy(t, w)).collect())
    }
}

/// Every line of `chart`, the saturation line first.
fn lines(chart: &Psychrometric) -> impl Iterator<Item = &ChartLine> {
    std::iter::once(&chart.saturation).chain(&chart.lines)
}

/// Every vertex is the batch's state at its inputs, bit for bit (`Exact`): on the saturation and φ lines its W at its T,
/// on the h, v and T_wb lines its T at its W; inside the range. Every run is at least two vertices long, and the chart
/// has the saturation line, φ's nine tenths and each of h, v and T_wb's lines.
#[test]
fn every_vertex_is_humid_airs_at_its_inputs() {
    let air = humid_air();
    let batch = AtP { air: &air, p: 101_325.0 };
    let range = ChartRange::default();
    let chart = psychrometric(&batch, range).unwrap();
    let count = |f: fn(&Held) -> bool| chart.lines.iter().filter(|l| f(&l.held)).count();
    assert_eq!(count(|h| matches!(h, Held::Relative(_))), 9);
    for f in [
        |h: &Held| matches!(h, Held::Enthalpy(_)),
        |h: &Held| matches!(h, Held::Volume(_)),
        |h: &Held| matches!(h, Held::WetBulb(_)),
    ] {
        assert!((3..=12).contains(&count(f)));
    }
    let mut vertices = 0;
    for line in lines(&chart) {
        assert!(!line.segments.is_empty() && line.segments.iter().all(|s| s.len() >= 2), "{:?}", line.held);
        let in_t = matches!(line.held, Held::Saturation | Held::Relative(_));
        for v in line.segments.iter().flatten() {
            let (got, want) =
                if in_t { (v.w, batch.ratio(line.held, v.t)) } else { (v.t, batch.temperature(line.held, v.w)) };
            assert_eq!(Some(got.to_bits()), want.map(f64::to_bits), "{:?} at {} K, W {}", line.held, v.t, v.w);
            assert!((range.t.0..=range.t.1).contains(&v.t) && (0.0..=range.w_max).contains(&v.w));
            vertices += 1;
        }
    }
    assert_eq!(vertices, 330);
}

/// The authors' Tables 1 and 2, compiled in from phasekit-verify's fixtures.
const TABLES_1_AND_2: [(&str, &str); 2] = [
    (
        "paper/HumidAir/Herrmann-HVACR-2009.1.csv",
        include_str!("../../phasekit-verify/fixtures/paper/HumidAir/Herrmann-HVACR-2009.1.csv"),
    ),
    (
        "paper/HumidAir/Herrmann-HVACR-2009.2.csv",
        include_str!("../../phasekit-verify/fixtures/paper/HumidAir/Herrmann-HVACR-2009.2.csv"),
    ),
];

/// Arbiter: the authors' Tables 1 and 2 (PLAN.md M18.9): where the saturation line has a vertex at a temperature of the
/// tables (the sweep starts on the range's ticks in °C), its W is the table's W_s within its printed digits, on the
/// default chart (0 to 30 °C below its top W of 0.03) and on one from −60 to 90 °C to W 1.5 (every 20 K).
#[test]
fn the_saturation_line_is_tables_1_and_2() {
    let air = humid_air();
    let batch = AtP { air: &air, p: 101_325.0 };
    let (mut checked, mut missed) = (0, Vec::new());
    for range in [ChartRange::default(), ChartRange { t: (213.15, 363.15), w_max: 1.5 }] {
        let chart = psychrometric(&batch, range).unwrap();
        for (path, text) in TABLES_1_AND_2 {
            let table = Fixture::parse(path, text).unwrap();
            for row in 0..table.rows().len() {
                let t = table.value(row, "t").unwrap() + 273.15;
                let Some(v) = chart.saturation.segments.iter().flatten().find(|v| v.t == t) else { continue };
                checked += 1;
                missed.extend(table.check(row, "Ws", v.w).err().map(|e| format!("{t} K: {e:?}")));
            }
        }
    }
    assert_eq!(missed, Vec::<String>::new());
    assert_eq!(checked, 26);
}

/// Every line of Mollier's `chart`, the saturation line first.
fn mollier_lines(chart: &Mollier) -> impl Iterator<Item = &ChartLine<MollierVertex>> {
    std::iter::once(&chart.saturation).chain(&chart.lines)
}

/// The state a Mollier vertex on a line of `held` stands for, from its inputs: on the saturation and φ lines W at its T
/// and h at (T, W); on an isotherm h at (T, W); on an h line T at (W, h); on a v line T at (W, v) and h at (T, W).
fn mollier_state(batch: &AtP, held: Held, v: &MollierVertex) -> Option<(f64, f64, f64)> {
    let (t, w) = match held {
        Held::Saturation | Held::Relative(_) => (v.t, batch.ratio(held, v.t)?),
        Held::DryBulb(t) => (t, v.w),
        _ => (batch.temperature(held, v.w)?, v.w),
    };
    let h = if let Held::Enthalpy(h) = held { h } else { batch.enthalpy(t, w)? };
    Some((t, w, h))
}

/// Every vertex of Mollier's chart is the batch's state at its inputs and is drawn at h − r₀W, bit for bit (`Exact`),
/// inside the range; every run is at least two vertices long; the chart has the saturation line, φ's nine tenths, an
/// isotherm at each of the range's ticks in °C, and h's and v's lines.
#[test]
fn every_mollier_vertex_is_humid_airs_at_its_inputs() {
    let air = humid_air();
    let batch = AtP { air: &air, p: 101_325.0 };
    let range = ChartRange::default();
    let chart = mollier(&batch, range).unwrap();
    let count = |f: fn(&Held) -> bool| chart.lines.iter().filter(|l| f(&l.held)).count();
    assert_eq!(count(|h| matches!(h, Held::Relative(_))), 9);
    assert_eq!(count(|h| matches!(h, Held::DryBulb(_))), 11);
    assert!((3..=12).contains(&count(|h| matches!(h, Held::Enthalpy(_)))));
    assert!((3..=12).contains(&count(|h| matches!(h, Held::Volume(_)))));
    let mut vertices = 0;
    for line in mollier_lines(&chart) {
        assert!(!line.segments.is_empty() && line.segments.iter().all(|s| s.len() >= 2), "{:?}", line.held);
        for v in line.segments.iter().flatten() {
            let bits = |(t, w, h): (f64, f64, f64)| [t.to_bits(), w.to_bits(), h.to_bits()];
            assert_eq!(Some(bits((v.t, v.w, v.h))), mollier_state(&batch, line.held, v).map(bits), "{:?}", line.held);
            assert_eq!(v.y.to_bits(), (v.h - chart.r0 * v.w).to_bits());
            assert!((range.t.0..=range.t.1).contains(&v.t) && (0.0..=range.w_max).contains(&v.w));
            vertices += 1;
        }
    }
    assert_eq!(vertices, 379);
}

/// Mollier's skew and its lines' meeting, at 1 atm: r₀ is water vapour's h at 0 °C in air, 2499.88 kJ/kg on liquid
/// water's zero at the triple point (the textbooks' ideal mixture has 2501; the virial terms take about 1 kJ/kg); the
/// 0 °C isotherm lies level, its ends' heights within a few ulps of its h at saturation of each other and every vertex
/// within 0.5 J/kg (2e-7 of r₀; 0.45 measured, the bow of the virial terms); each isotherm that reaches saturation
/// inside the range (0 to 30 °C) ends on the saturation line's vertex at its T; and wherever M18.9's saturation line and
/// Mollier's share a T (16 of them), their W is the same.
#[test]
fn mollier_s_0_c_isotherm_is_level_and_its_lines_meet() {
    let air = humid_air();
    let batch = AtP { air: &air, p: 101_325.0 };
    let range = ChartRange::default();
    let chart = mollier(&batch, range).unwrap();
    assert!((2.4995e6..2.5005e6).contains(&chart.r0), "{}", chart.r0);
    let saturation: Vec<&MollierVertex> = chart.saturation.segments.iter().flatten().collect();
    let mut ended = 0;
    for line in &chart.lines {
        let Held::DryBulb(t) = line.held else { continue };
        let last = line.segments.last().and_then(|s| s.last()).unwrap();
        if t == 273.15 {
            let ys: Vec<f64> = line.segments.iter().flatten().map(|v| v.y).collect();
            let (first, spread) = (ys[0], ys.iter().map(|y| (y - ys[0]).abs()).fold(0.0, f64::max));
            assert!(
                (last.y - first).abs() <= 4.0 * f64::EPSILON * last.h && spread <= 0.5,
                "{first} {} {spread}",
                last.y
            );
        }
        if last.w < range.w_max {
            assert!(saturation.iter().any(|s| s.t == t && s.w == last.w && s.y == last.y), "{t} K");
            ended += 1;
        }
    }
    assert_eq!(ended, 7);
    let psychro = psychrometric(&batch, range).unwrap();
    let shared: Vec<_> = psychro
        .saturation
        .segments
        .iter()
        .flatten()
        .filter_map(|p| saturation.iter().find(|m| m.t == p.t).map(|m| (p.w, m.w)))
        .collect();
    assert!(shared.iter().all(|(a, b)| a == b));
    assert_eq!(shared.len(), 16);
}
