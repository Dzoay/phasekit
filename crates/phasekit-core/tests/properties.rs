//! Property tests (PLAN.md M1.14; ROT-006, ROT-012, ROT-022): the raw input gate and the batch driver take any value
//! and any buffer shape without panicking. The seed is fixed, so CI and every machine run the same cases; a failure
//! proptest finds is recorded in `proptest-regressions/properties.txt` and replayed first from then on.

use std::sync::{Arc, LazyLock};

use phasekit_core::batch::{self, BatchRequest, Status};
use phasekit_core::internal::FluidRecord;
use phasekit_core::{Basis, Error, FlashOptions, Fluid, Input, Pair, Phase, Prop, Var};
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

/// 4096 cases from a fixed seed (PLAN.md M1.14). Failures go to `proptest-regressions/properties.txt` under the
/// crate root, where cargo runs tests (proptest's default looks for a `src/lib.rs` beside the test and, for a file
/// under `tests/`, finds none).
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 4096,
        rng_seed: RngSeed::Fixed(0x7068_6173_656b_6974),
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct("proptest-regressions/properties.txt"))),
        ..ProptestConfig::default()
    }
}

/// Any f64 bit pattern, with the edges a uniform draw over bits rarely hits.
fn any_f64() -> impl Strategy<Value = f64> {
    prop_oneof![
        8 => any::<u64>().prop_map(f64::from_bits),
        1 => prop::sample::select(vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::MAX,
            f64::NAN,
            -f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ]),
        1 => -1e3..1e3_f64,
    ]
}

/// Every pair, either basis.
fn pair_and_basis() -> impl Strategy<Value = (Pair, Basis)> {
    (
        prop::sample::select(Pair::ALL.to_vec()),
        prop::bool::ANY.prop_map(|mass| if mass { Basis::Mass } else { Basis::Molar }),
    )
}

/// The rule of each variable's newtype (map 01 R2-R4): T, p and ρ finite and positive, Q in [0, 1], h, s and u
/// finite. Written out here, independently of `Var`'s own table, as the property's oracle.
fn valid(var: Var, v: f64) -> bool {
    match var {
        Var::T | Var::P | Var::D => v.is_finite() && v > 0.0,
        Var::Q => (0.0..=1.0).contains(&v),
        _ => v.is_finite(),
    }
}

proptest! {
    #![proptest_config(config())]

    /// ROT-012: the one raw gate accepts exactly the values both newtypes accept, keeps them bit for bit, and refuses
    /// every other value with `InvalidInput`: NaN, ±inf, non-positive T, p and ρ, Q outside [0, 1].
    #[test]
    fn input_new_never_panics(x in any_f64(), y in any_f64(), (pair, basis) in pair_and_basis()) {
        let (vx, vy) = pair.vars();
        match Input::new(pair, x, y, basis) {
            Ok(input) => {
                prop_assert!(valid(vx, x) && valid(vy, y), "{pair:?} accepted ({x:e}, {y:e})");
                let (gx, gy) = input.values();
                prop_assert_eq!((input.pair(), gx.to_bits(), gy.to_bits()), (pair, x.to_bits(), y.to_bits()));
            }
            Err(Error::InvalidInput { value, .. }) => {
                prop_assert!(!(valid(vx, x) && valid(vy, y)), "{pair:?} refused ({x:e}, {y:e})");
                let refused = if valid(vx, x) { y } else { x };
                prop_assert_eq!(value.to_bits(), refused.to_bits(), "the error names the refused value");
            }
            Err(e) => prop_assert!(false, "{pair:?} ({x:e}, {y:e}): {e:?}"),
        }
    }

    /// E12, S-06, ROT-022: any (points, outputs) shape and any buffer lengths are either evaluated, one status per
    /// cell with NaN exactly beside the failed cells, or refused with `Shape`; zero points, zero outputs and a cell
    /// count that overflows `usize` (reachable on wasm32) included.
    #[test]
    fn batch_request_shapes_never_panic(shape in shapes(), values in prop::collection::vec(any_f64(), 16)) {
        let Shape { n, m, y_len, cells } = shape;
        let big = n > SMALL;
        let x = if big { BIG_X.get(..n) } else { values.get(..n) };
        let y = if big { BIG_X.get(..y_len) } else { values.get(8..8 + y_len) };
        let outputs = if big { BIG_OUTPUTS.get(..m) } else { OUTPUTS.get(..m) };
        let (Some(x), Some(y), Some(outputs)) = (x, y, outputs) else { return Err(TestCaseError::fail("strategy")) };
        let req = BatchRequest::new(Pair::DT, Basis::Molar, x, y, outputs).with_flash(LIQUID);
        prop_assert_eq!(req.cells(), n.checked_mul(m));
        let (mut out, mut status) = (vec![7.0; cells], vec![Status::Other; cells]);
        let fits = y_len == n && n.checked_mul(m) == Some(cells);
        let fluid = FLUID.as_ref().map_err(|e| TestCaseError::fail(e.to_string()))?;
        match batch::evaluate(fluid, &req, &mut out, &mut status) {
            Ok(summary) => {
                prop_assert!(fits, "evaluated a mismatched shape");
                let failed = status.iter().filter(|s| !matches!(s, Status::Ok | Status::Extrapolated)).count();
                prop_assert_eq!((summary.points, summary.failed_cells), (n, failed));
                for (v, s) in out.iter().zip(&status) {
                    prop_assert_eq!(v.is_nan(), !matches!(s, Status::Ok | Status::Extrapolated), "{} {:?}", v, s);
                }
            }
            Err(Error::Shape { .. }) => prop_assert!(!fits, "refused a matching shape"),
            Err(e) => prop_assert!(false, "{e:?}"),
        }
    }
}

/// Small shapes are evaluated; above this many points a shape is big, built to overflow `usize` on 32-bit targets.
const SMALL: usize = 8;

/// One batch shape: points, outputs, the length of `y` and of the output and status buffers.
#[derive(Clone, Copy, Debug)]
struct Shape {
    n: usize,
    m: usize,
    y_len: usize,
    cells: usize,
}

/// Mostly small shapes, right or off by one; sometimes 2¹⁶ + k points × 2¹⁶ + j outputs, whose cell count overflows a
/// 32-bit `usize` (on 64-bit the buffers, never that long, are refused as mismatched).
fn shapes() -> impl Strategy<Value = Shape> {
    let off = || prop::sample::select(vec![0_isize, 0, 0, -1, 1]);
    let small = (0..=SMALL, 0..=OUTPUTS.len(), off(), off()).prop_map(|(n, m, dy, dc)| Shape {
        n,
        m,
        y_len: n.saturating_add_signed(dy).min(SMALL),
        cells: (n * m).saturating_add_signed(dc),
    });
    let big = (0..64_usize, 0..64_usize, 0..4_usize).prop_map(|(k, j, cells)| Shape {
        n: (1 << 16) + k,
        m: (1 << 16) + j,
        y_len: (1 << 16) + k,
        cells,
    });
    prop_oneof![15 => small, 1 => big]
}

const OUTPUTS: [Prop; 4] = [Prop::P, Prop::Hmolar, Prop::Cpmolar, Prop::Q];
const LIQUID: FlashOptions = FlashOptions::new().with_phase(Phase::Liquid);

static FLUID: LazyLock<Result<Fluid, Error>> =
    LazyLock::new(|| FluidRecord::synthetic("X").and_then(|r| r.compile()).map(|model| Fluid::new(Arc::new(model))));
static BIG_X: LazyLock<Vec<f64>> = LazyLock::new(|| vec![300.0; (1 << 16) + 64]);
static BIG_OUTPUTS: LazyLock<Vec<Prop>> = LazyLock::new(|| vec![Prop::T; (1 << 16) + 64]);
