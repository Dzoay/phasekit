//! The PropsSI-style facade over real fluids (PLAN.md M5.9; D11): strings reach the same flash and property table as
//! the typed API, bit for bit.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_compat::props_si_in;
use phasekit_core::{Basis, DataSet, Density, Input, Registry, Temperature};
use phasekit_verify::{Fixture, fixture};

/// Oracle: CoolProp 8.0.0, the core subset's `flash` DT rows (PLAN.md M5.9). `PropsSI(X, "T", T, "Dmolar", ρ, fluid)`
/// and the swapped `PropsSI(X, "Dmolar", ρ, "T", T, fluid)` for X = P, Hmolar, Smolar, Hmass are the typed DT flash's
/// values bit for bit, refusals included, and that flash matches the oracle on these rows (`dt_two_phase_matches_oracle`
/// in tests/flash.rs, class `Flash`); a direct comparison of p closes the chain within class `Flash`.
#[test]
fn props_si_dt_matches_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let files = [
        fixture!("coolprop-8.0.0/flash/Water.csv"),
        fixture!("coolprop-8.0.0/flash/CarbonDioxide.csv"),
        fixture!("coolprop-8.0.0/flash/R410A.csv"),
    ];
    let (mut answered, mut refused) = (0, 0);
    for (path, text) in files {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let fluid = registry.get(name).unwrap();
        for row in 0..fixture.rows().len() {
            let (rho, t) = (fixture.value(row, "x1").unwrap(), fixture.value(row, "x2").unwrap());
            let typed = fluid.state(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap()));
            for (key, prop) in [
                ("P", phasekit_core::Prop::P),
                ("Hmolar", phasekit_core::Prop::Hmolar),
                ("Smolar", phasekit_core::Prop::Smolar),
                ("Hmass", phasekit_core::Prop::Hmass),
            ] {
                let want = typed.as_ref().map_err(Clone::clone).and_then(|s| fluid.prop(s, prop));
                let got = props_si_in(&registry, key, "T", t, "Dmolar", rho, name).map_err(|e| format!("{e}"));
                let swapped = props_si_in(&registry, key, "Dmolar", rho, "T", t, name).map_err(|e| format!("{e}"));
                let want = want.map_err(|e| format!("{e}"));
                assert_eq!(
                    got.as_ref().map(|v| v.to_bits()),
                    want.as_ref().map(|v| v.to_bits()),
                    "{path} row {row} {key}"
                );
                assert_eq!(swapped, got, "{path} row {row} {key}, inputs swapped");
                if got.is_ok() { answered += 1 } else { refused += 1 }
            }
            if let Ok(state) = &typed {
                let (oracle, p) = (fixture.value(row, "p").unwrap(), state.p());
                let bound = phasekit_verify::ToleranceClass::Flash.bound(oracle.abs()).unwrap() * 1e3;
                assert!((p - oracle).abs() <= bound, "{path} row {row}: p {p} against {oracle}");
                let _ = state.h(Basis::Molar);
            }
        }
    }
    // 3 × 52 rows, 4 keys each, R410A's dome rows too since M6.9 (none is refused any more).
    assert_eq!((answered, refused), (3 * 52 * 4, 0));
}
