//! `humidair_hapropssi` (PLAN.md M18.12): compat's `HAPropsSI` over humid air under `Corrected`, the calls map 07 §3.3
//! timed on v8.0.0's (H(T, R) 7.5 µs, W(T, R) 6.8 µs, T_dp 132 µs, T_wb 152 µs, T(H, R) 203 µs a call): at 1 atm on a
//! grid of SplitMix64 seed 1, T ~ U[273.15, 323.15] K and φ ~ U[0.1, 0.9]; T(H, R) at each point's own h. Recorded by
//! `cargo xtask bench --record`.

#[cfg(not(target_family = "wasm"))]
mod bench {
    use criterion::{BenchmarkId, Criterion, criterion_group};
    use phasekit_compat::{ha_props_si_in, humid_air_in};
    use phasekit_core::math::BACKEND;
    use phasekit_core::{DataSet, Registry};
    use phasekit_verify::SplitMix64;
    use phasekit_verify::backend::case;

    /// Points on the grid, cycled through one per iteration.
    const POINTS: usize = 1024;

    /// The pressure, Pa.
    const P: f64 = 101_325.0;

    fn humidair_hapropssi(c: &mut Criterion) {
        let Ok(registry) = Registry::from_embedded(DataSet::Corrected) else { return };
        let Ok(air) = humid_air_in(&registry, DataSet::Corrected) else { return };
        let mut rng = SplitMix64::new(1);
        let grid: Vec<(f64, f64)> = (0..POINTS).map(|_| (rng.uniform(273.15, 323.15), rng.uniform(0.1, 0.9))).collect();
        let h: Vec<f64> =
            grid.iter().map(|&(t, r)| ha_props_si_in(&air, "H", "T", t, "P", P, "R", r).unwrap_or(f64::NAN)).collect();
        let mut g = c.benchmark_group("humidair_hapropssi");
        for output in ["H", "W", "Tdp", "Twb"] {
            g.bench_function(BenchmarkId::new(output, case(BACKEND, "t_r")), |b| {
                let mut k = 0;
                b.iter(|| {
                    k = (k + 1) % grid.len();
                    let (t, r) = std::hint::black_box(grid[k]);
                    std::hint::black_box(ha_props_si_in(&air, output, "T", t, "P", P, "R", r))
                });
            });
        }
        g.bench_function(BenchmarkId::new("T", case(BACKEND, "h_r")), |b| {
            let mut k = 0;
            b.iter(|| {
                k = (k + 1) % grid.len();
                let (h, r) = std::hint::black_box((h[k], grid[k].1));
                std::hint::black_box(ha_props_si_in(&air, "T", "H", h, "P", P, "R", r))
            });
        });
        g.finish();
    }

    criterion_group!(benches, humidair_hapropssi);
}

#[cfg(not(target_family = "wasm"))]
criterion::criterion_main!(bench::benches);
