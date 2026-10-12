//! The sweeps' kit (PLAN.md M7.9d): the Cartesian product of two case lists, so that a sweep is one loop with a named
//! function as its body, and a collector of the failures a sweep finds, which its test asserts empty once. A test
//! binary includes it with `#[path = "common/sweep.rs"] mod sweep;` and uses its part of it.

#![allow(dead_code, reason = "each test binary that includes the kit uses its part of it")]

use std::fmt;
use std::ops::{Deref, DerefMut};

/// Every pair of an item of `a` and an item of `b`, `a` outermost: the cases of `for x in a { for y in b { … } }`, in
/// that order.
pub fn product<A: Clone, B: Clone>(a: &[A], b: &[B]) -> impl Iterator<Item = (A, B)> {
    a.iter().flat_map(move |x| b.iter().map(move |y| (x.clone(), y.clone())))
}

/// A sweep's failures, one message each: the sweep records every failing case and its test asserts that there is
/// none, so that one run reports them all. Otherwise it is the `Vec<String>` it holds (`push`, `is_empty`, `join`).
#[derive(Default)]
pub struct Failures(Vec<String>);

impl Failures {
    /// Records `message()` unless `ok`.
    pub fn check(&mut self, ok: bool, message: impl FnOnce() -> String) {
        if !ok {
            self.0.push(message());
        }
    }

    /// The value of `result`, or `None` with `message(error)` recorded.
    pub fn ok<T, E>(&mut self, result: Result<T, E>, message: impl FnOnce(E) -> String) -> Option<T> {
        result.map_err(|e| self.0.push(message(e))).ok()
    }
}

impl Deref for Failures {
    type Target = Vec<String>;

    fn deref(&self) -> &Vec<String> {
        &self.0
    }
}

impl DerefMut for Failures {
    fn deref_mut(&mut self) -> &mut Vec<String> {
        &mut self.0
    }
}

/// As the `Vec<String>` it holds, so that a test's message lists the failures as a vector's would.
impl fmt::Debug for Failures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
