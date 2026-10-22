//! The cross-target determinism hash (PLAN.md M9.4; VERIFICATION.md §9.4, the grid's one definition): every embedded
//! fluid's canonical grid through the scalar path, hashed bit for bit. Run on each target with std maths and with
//! `--features phasekit-core/libm`; it prints `hash=<hex> target=<arch-os> math=<std|libm>`.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::batch::Status;
use phasekit_core::{
    Basis, Density, Enthalpy, Error, Fluid, Input, Order, Pressure, Quality, Registry, State, Temperature, math,
};
use phasekit_verify::{SplitMix64, fixture};

/// The hash every target reproduces with `libm` (VERIFICATION.md §9.4): the four agreed at M9.4.
const LIBM: (&str, &str) = fixture!("hash/libm.txt");
/// The hash every target reproduces with `fast`, which fuses a·b + c in hardware or in software (PLAN.md M23.7).
const FAST: (&str, &str) = fixture!("hash/fast.txt");

/// FNV-1a 64, fed byte by byte.
struct Fnv(u64);

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    /// An output: its bits, little-endian, NaN canonical, then its status byte.
    fn cell(&mut self, value: &Result<f64, Error>) {
        let v = value.as_ref().map_or(f64::NAN, |v| *v);
        self.bytes(&if v.is_nan() { f64::NAN } else { v }.to_bits().to_le_bytes());
        self.bytes(&[status_byte(value)]);
    }

    /// p, h, s, c_v, c_p and w of a flash result, each a cell.
    fn state(&mut self, state: &Result<State, Error>) -> bool {
        let m = Basis::Molar;
        let cells: [Result<f64, Error>; 6] = match state {
            Ok(s) => [Ok(s.p()), Ok(s.h(m)), Ok(s.s(m)), s.cv(m), s.cp(m), s.speed_of_sound()],
            Err(e) => [(); 6].map(|()| Err(e.clone())),
        };
        cells.iter().for_each(|cell| self.cell(cell));
        state.is_ok()
    }
}

/// A fixed byte per outcome (the hash's own: `Status` discriminants are not an ABI).
fn status_byte(value: &Result<f64, Error>) -> u8 {
    match value.as_ref().map_err(Status::from) {
        Ok(_) => 0,
        Err(Status::InvalidInput) => 1,
        Err(Status::Unsupported) => 2,
        Err(Status::Domain) => 3,
        Err(Status::NoConvergence) => 4,
        Err(Status::Ambiguous) => 5,
        Err(Status::Undefined) => 6,
        Err(Status::NoModel) => 7,
        Err(Status::Load) => 8,
        Err(_) => 9,
    }
}

/// One fluid's grid into `hash` (VERIFICATION.md §9.4), SplitMix64 seed 1: 64 (T, ρ) on the `eos` grid, T ~ U[T_low,
/// T_max] and ρ ~ logU[1e-6·ρ_L, ρ_L] with ρ_L the saturated liquid at T_low = max(T_min, T_triple) (3ρ_c where QT has
/// none), each with its order-2 α^r bundle; 16 QT and 16 PQ on the `sat` grid, Θ = 1 − T/T_c ~ logU[1e-7, 1 −
/// T_low/T_c] with Q 0 and 1 in turn, PQ at the QT state's p; 16 PT and 16 PH read off DT truths of the `eos` grid.
/// Returns the grid points and the flashes that succeeded.
fn fluid_grid(hash: &mut Fnv, registry: &Registry, fluid: &Fluid, name: &str) -> (usize, usize) {
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let limits = &record.limits;
    let t_low = limits.t_min().max(limits.t_triple().unwrap_or(limits.t_min()));
    let crit = record.critical.unwrap();
    let qt = |q: f64, t: f64| Some(Input::qt(Quality::new(q).ok()?, Temperature::new(t).ok()?));
    let liquid = qt(0.0, t_low).and_then(|input| fluid.state(input).ok());
    let rho_l = liquid.map_or(3.0 * crit.rho, |s| s.rho(Basis::Molar));
    let dt = |rho: f64, t: f64| Some(Input::dt(Density::molar(rho).ok()?, Temperature::new(t).ok()?));
    let flash = |input: Option<Input>| input.ok_or(Error::InvalidArgument { argument: "grid input", value: None });
    let mut rng = SplitMix64::new(1);
    let eos_point = |rng: &mut SplitMix64| (rng.log_uniform(1e-6 * rho_l, rho_l), rng.uniform(t_low, limits.t_max()));
    let (mut points, mut ok) = (0, 0);
    let mut count = |hash: &mut Fnv, state: Result<State, Error>| {
        points += 1;
        ok += usize::from(hash.state(&state));
        state.ok()
    };
    let helmholtz = fluid.model().helmholtz().unwrap();
    for _ in 0..64 {
        let (rho, t) = eos_point(&mut rng);
        count(hash, flash(dt(rho, t)).and_then(|input| fluid.state(input)));
        let bundle = helmholtz.residual(t, rho, Order::Two);
        for n in 0..=2 {
            for i in 0..=n {
                hash.cell(&Ok(bundle.get(i, n - i).unwrap_or(f64::NAN)));
            }
        }
    }
    let theta_max = 1.0 - t_low / crit.t;
    for k in 0..16 {
        let (t, q) = (crit.t * (1.0 - rng.log_uniform(1e-7, theta_max)), (k % 2) as f64);
        let sat = count(hash, flash(qt(q, t)).and_then(|input| fluid.state(input)));
        let p = sat.map_or(f64::NAN, |s| s.p());
        let pq = Pressure::new(p).ok().zip(Quality::new(q).ok()).map(|(p, q)| Input::pq(p, q));
        count(hash, flash(pq).and_then(|input| fluid.state(input)));
    }
    for _ in 0..16 {
        let (rho, t) = eos_point(&mut rng);
        let truth = flash(dt(rho, t)).and_then(|input| fluid.state(input)).ok();
        let (p, h) = truth.map_or((f64::NAN, f64::NAN), |s| (s.p(), s.h(Basis::Molar)));
        let pt = Pressure::new(p).ok().zip(Temperature::new(t).ok()).map(|(p, t)| Input::pt(p, t));
        count(hash, flash(pt).and_then(|input| fluid.state(input)));
        let ph = Enthalpy::molar(h).ok().zip(Pressure::new(p).ok()).map(|(h, p)| Input::hp(h, p));
        count(hash, flash(ph).and_then(|input| fluid.state(input)));
    }
    (points, ok)
}

/// VERIFICATION.md §9.4 (PLAN.md M9.4): the canonical grid of all 136 embedded fluids, 128 points each, hashed.
/// Printed for the CI job that collects it from x86_64 Linux, Windows MSVC, aarch64 Linux and wasip2, with std, `libm`
/// and `fast`; std hashes are reported, never asserted. Asserted: the committed `fixtures/hash/libm.txt` with `libm` and
/// `fixtures/hash/fast.txt` with `fast` (PLAN.md M23.7), on every target (class `Exact`); every fluid hashed its 128
/// points; and at least three in four of the flashes succeeded, so the hash covers answers, not only refusals.
#[test]
fn canonical_grid_hash() {
    let registry = Registry::embedded().unwrap();
    let mut hash = Fnv(0xcbf2_9ce4_8422_2325);
    let (mut points, mut ok) = (0, 0);
    for f in phasekit_data::FLUIDS {
        hash.bytes(f.name.as_bytes());
        let (p, o) = fluid_grid(&mut hash, registry, registry.get(f.name).unwrap(), f.name);
        (points, ok) = (points + p, ok + o);
    }
    let os = if cfg!(target_family = "wasm") { "wasi" } else { std::env::consts::OS }; // empty under wasip2
    let target = format!("{}-{os}", std::env::consts::ARCH);
    let line = format!("hash={:016x} target={target} math={}", hash.0, math::BACKEND);
    #[allow(clippy::print_stdout, reason = "the hash line is this test's record, collected by CI")]
    {
        println!("\n{line}");
    }
    assert_eq!(points, phasekit_data::FLUIDS.len() * 128, "{line}");
    assert!(4 * ok >= 3 * points, "{ok} of {points} flashes succeeded: {line}");
    let committed = match math::BACKEND {
        "libm" => Some(LIBM),
        "fast" => Some(FAST),
        _ => None,
    };
    if let Some((name, text)) = committed {
        let want = text.lines().find(|l| !l.starts_with('#')).unwrap();
        assert_eq!(format!("{:016x}", hash.0), want, "{line} against {name}");
    }
}
