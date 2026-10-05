//! A seeded, logged, hand-written sampler for Rust-side grids (dependencies §2.12): SplitMix64 (`splitmix64-v1` in
//! `oracle.lock`), reproducible bit for bit on every target, wasip2 included. The oracle generator implements the same
//! generator and checks the same golden vector at start-up (VERIFICATION.md §3.1, assertion 5).

use phasekit_core::math;

/// The SplitMix64 generator: a 64-bit state advanced by the golden-ratio increment and mixed by two multiplies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator starting from `seed` (fixtures use seed 1 unless their `grid:` line says otherwise).
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uniform draw in [0, 1): the top 53 bits times 2⁻⁵³, exact in binary64.
    pub fn next_f64(&mut self) -> f64 {
        // 2^53 < 2^64: the cast of the top 53 bits is exact, and so is the scaling by a power of two.
        (self.next_u64() >> 11) as f64 * math::powi(2.0, -53)
    }

    /// A uniform draw in [lo, hi): `lo + (hi − lo)·u`, the same operations the generator script performs.
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// A log-uniform draw in [lo, hi] for 0 < lo ≤ hi: uniform in ln, through the `math` choke point.
    pub fn log_uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let (ln_lo, ln_hi) = (math::ln(lo), math::ln(hi));
        // exp can round a hair past either end; the draw stays inside [lo, hi].
        math::exp(ln_lo + (ln_hi - ln_lo) * self.next_f64()).clamp(lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first four outputs for seed 1, the vector gen.py asserts at start-up (VERIFICATION.md §3.1, assertion 5);
    /// computed independently in Python.
    const GOLDEN: [u64; 4] =
        [0x910a_2dec_8902_5cc1, 0xbeeb_8da1_658e_ec67, 0xf893_a2ee_fb32_555e, 0x71c1_8690_ee42_c90b];

    #[test]
    fn splitmix64_seed_1_golden_vector() {
        let mut rng = SplitMix64::new(1);
        assert_eq!([rng.next_u64(), rng.next_u64(), rng.next_u64(), rng.next_u64()], GOLDEN);
        assert_ne!(SplitMix64::new(2).next_u64(), GOLDEN[0], "the seed matters");
    }

    /// Python's `(x >> 11) * 2.0**-53` and `lo + (hi - lo) * u` give these bits, so the grids agree across languages.
    #[test]
    fn uniform_draws_match_the_generator_script() {
        let mut rng = SplitMix64::new(1);
        let unit = [0x3fe2_2145_bd91_204b, 0x3fe7_dd71_b42c_b1dd, 0x3fef_1274_5ddf_664a, 0x3fdc_7061_a43b_90b2];
        assert_eq!([0; 4].map(|_| rng.next_f64().to_bits()), unit);
        let mut rng = SplitMix64::new(1);
        let draws = [0; 4].map(|_| rng.uniform(200.0, 650.0));
        assert_eq!(draws, [454.9527088275264, 535.6017907682156, 636.9512391140584, 399.9616476750974]);
        let mut rng = SplitMix64::new(1);
        assert!((0..10_000).all(|_| (0.0..1.0).contains(&rng.next_f64())));
    }

    /// Log-uniform draws spread over decades: about half fall below the geometric mean of [1e-6, 1] (Python: 537 of
    /// 1000 for seed 1), where uniform draws put one in a thousand.
    #[test]
    fn log_uniform_draws_spread_over_decades() {
        let mut rng = SplitMix64::new(1);
        let draws: Vec<f64> = (0..1000).map(|_| rng.log_uniform(1e-6, 1.0)).collect();
        assert!(draws.iter().all(|v| (1e-6..=1.0).contains(v)), "{draws:?}");
        let below = draws.iter().filter(|v| **v < 1e-3).count();
        assert!((500..575).contains(&below), "{below} of 1000 below the geometric mean");
        let mut rng = SplitMix64::new(1);
        assert_eq!((0..1000).filter(|_| rng.uniform(1e-6, 1.0) < 1e-3).count(), 1);
        assert_eq!(SplitMix64::new(7).log_uniform(5.0, 5.0), 5.0);
    }
}
