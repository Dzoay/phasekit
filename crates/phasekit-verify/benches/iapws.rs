//! `if97_pt` (PLAN.md M14.3; ROT-243): IF97 water's PT flash plus h and c_p, one Gibbs evaluation a state through
//! `bundle_from_gibbs`, on a grid of each region: SplitMix64 seed 1, T ~ U[273.15, 623.15] K, p ~ log-uniform over
//! [1 kPa, 100 MPa] and kept in the region (above or below Eq. (30)'s saturation pressure). v8.0.0's IF97 costs 750 ns
//! a point by its `fast_evaluate` (map 07 R8). Recorded by `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_core::math::{self, BACKEND};
    use phasekit_core::{Basis, Input, Pressure, Temperature};
    use phasekit_iapws::if97;
    use phasekit_verify::SplitMix64;
    use phasekit_verify::backend::case;

    /// States per region, cycled through one per iteration.
    const STATES: usize = 1024;

    /// The region's PT inputs: region 1 where `liquid`, else region 2.
    fn grid(liquid: bool) -> Vec<Input> {
        let mut rng = SplitMix64::new(1);
        let mut inputs = Vec::with_capacity(STATES);
        while inputs.len() < STATES {
            let (t, p) = (rng.uniform(273.15, 623.15), math::powf(10.0, rng.uniform(3.0, 8.0)));
            let Ok(p_s) = if97::saturation_pressure(t) else { continue };
            if (p > p_s) == liquid
                && let (Ok(p), Ok(t)) = (Pressure::new(p), Temperature::new(t))
            {
                inputs.push(Input::pt(p, t));
            }
        }
        inputs
    }

    fn if97_pt(c: &mut Criterion) {
        let Ok(water) = if97::water() else { return };
        let mut g = c.benchmark_group("if97_pt");
        for (region, liquid) in [("region 1", true), ("region 2", false)] {
            let states = grid(liquid);
            g.bench_function(BenchmarkId::new(region, case(BACKEND, "pt_h_cp")), |b| {
                let mut k = 0;
                b.iter(|| {
                    k = (k + 1) % states.len();
                    let state = water.state(std::hint::black_box(states[k]));
                    std::hint::black_box(state.map(|s| (s.h(Basis::Mass), s.cp(Basis::Mass))))
                });
            });
        }
        g.finish();
    }

    criterion_group!(benches, if97_pt);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);
