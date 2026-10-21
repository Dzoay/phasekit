//! The psychrometric chart (PLAN.md M18.9): every vertex is humid air's W at its T and the line's held quantity, bit
//! for bit (`Exact`); where the saturation line passes a temperature of the authors' Tables 1 and 2, its W is the
//! table's within its printed digits; the families and their runs keep to the range.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{DataSet, Error, Registry};
use phasekit_diagram::{ChartLine, ChartRange, Held, HumidAirBatch, Psychrometric, psychrometric};
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
