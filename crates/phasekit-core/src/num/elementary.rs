//! Core's own elementary functions (PLAN.md M23; user decisions IM1-IM5): each one scalar function under 1 ulp, with a
//! main path free of branches and divisions, a test for whether a value takes it, and selects for the special cases,
//! so that its lane form maps the same function across [`Lanes`](super::Lanes) and each lane is the scalar path's bits
//! by construction (K17). With the `libm` feature `math` and `Lanes` run them; `internal` exposes the scalar functions
//! in every build, for the tests against the reference values (`phasekit-verify`'s `fixtures/math/`).

mod exp;

pub use exp::exp;
#[cfg(feature = "libm")]
pub(crate) use exp::exp_lanes;
