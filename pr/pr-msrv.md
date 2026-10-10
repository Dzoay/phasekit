## What and why

User decision **MS1** (2026-10-10, recorded in `docs/design/04-user-decisions.md`): the workspace MSRV goes from 1.85 to **1.89**, and the code uses what 1.89 allows.

- **Why.**
  - The MSRV never limited tooling: development, lints and CI already use the latest stable (1.99.0). It limits only the language and library features the source may use.
  - D17 raises it only for a concrete feature, and there are several:
    - let-chains (1.88);
    - `slice::as_chunks` (1.88);
    - `f64::next_up`/`next_down` (1.86);
    - every test and dev-dependency building on the MSRV, including num-dual (1.89).
  - M12's SIMD needed 1.89 anyway.
  - 1.85 was the edition-2024 floor, Debian 13's packaged rustc and the Linux kernel's minimum. Users build with rustup, common practice in Rust; Debian's packaged 1.85 is not a target.
- **Toolchain.**
  - `rust-version = "1.89"`, and `scripts/check-toolchain.sh` asserts it.
  - The `msrv` CI job installs 1.89.0 (with clippy, for the lint probe test) and runs G3 and the doctests on it. Before, it only ran `cargo check --lib` on 1.85.0, because the dev-dependencies needed newer toolchains. cargo-nextest is built with the stable toolchain and cached.
- **The code** (`refactor: the code Rust 1.89 allows`). Clippy reads `rust-version`, so at 1.89 its MSRV-gated suggestions apply:
  - let-chains where nested `if`s collapse: the region rule's critical-point check, the VLE's line search, four in xtask, one in a test;
  - let-chains in place of the tuple patterns that stood in for them (`if let (false, Some(slot)) = (seen, …)`): `saturation.rs`, `gates/counts.rs`, `main.rs`;
  - `as_chunks` for constant-size chunks: the blob reader's f64s (no more `try_into().unwrap_or_default()`) and SHA-256's blocks and words;
  - `next_up`/`next_down` for one-ulp neighbours in tests;
  - datagen's one-bit flip of a stored float stays a bit operation, beside the integers' `^= 1`.
- **Docs.** D17 (ARCHITECTURE.md), PLAN.md §2 and M0.6, M3's prerequisite note and M12, VERIFICATION.md §11.3, and the research note's open question 1, now answered.

## Tests

| Check | Result |
|---|---|
| `cargo +1.89.0 nextest run --workspace --profile ci` | 423 passed (all) |
| `cargo +1.89.0 test --doc --workspace` | passed |
| `cargo +1.89.0 check --workspace --all-targets` | clean |
| clippy (`-D warnings`, MSRV-aware at 1.89) | clean after the refactor; 11 findings before it |

## Red evidence

This is a toolchain change with no failing test. With `rust-version = "1.89"`, clippy at `-D warnings` failed on 11 findings: 7 collapsible `if`s, now let-chains, and 4 constant-size `chunks_exact`, now `as_chunks`. The refactor commit fixes them.

## Gates

GATES

🤖 Generated with [Claude Code](https://claude.com/claude-code)
