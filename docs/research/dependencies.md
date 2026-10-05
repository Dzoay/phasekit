# Minimal-dependency building blocks and OSS hygiene

Research input for the coolprop-rs **dependency policy**. It also touches D9 (execution strategies), D11 (facades and units) and D14 (licensing).
Date: 2026-10-04.

- Crate versions, release dates, licences and declared `rust-version` come from the crates.io API and the crates.io sparse index on that date. Each crate links to its crates.io page.
- Toolchain facts come from Rust's [RELEASES.md](https://github.com/rust-lang/rust/blob/master/RELEASES.md).
- Local CoolProp citations are paths under `reference/CoolProp` at v8.0.0 (`ae81610e`).
- Dependency counts, the compile-cost proxy and the compression figures were computed for this document. §6 describes the methods.

**Bottom line**

- The default build of every published core crate should have **zero third-party dependencies**. This is realistic in Rust 1.99:
  - std provides lazy statics (`OnceLock`, `LazyLock`), scoped threads, safe `#[target_feature]` functions, safe `std::arch` intrinsics and runtime CPU detection;
  - each remaining need (dual numbers, small LU/Cholesky, a binary data reader, error enums, SI newtypes) is a few hundred lines that we control.
- Only four crates earn an **opt-in** place next to the core:
  - [`rayon`](https://crates.io/crates/rayon) for parallel batches;
  - [`serde`](https://crates.io/crates/serde) + [`serde_json`](https://crates.io/crates/serde_json) for CoolProp-JSON import;
  - [`libm`](https://crates.io/crates/libm) for bit-reproducible math;
  - [`fearless_simd`](https://crates.io/crates/fearless_simd) for explicit SIMD, in its own backend crate.
- Binding crates ([`pyo3`](https://crates.io/crates/pyo3), [`wasm-bindgen`](https://crates.io/crates/wasm-bindgen), [`wit-bindgen`](https://crates.io/crates/wit-bindgen)) live only in facade crates.
- Do not design around nightly Rust. `std::simd`, `std::autodiff` and transcendental math in `core` are all unstable in 1.99, and none of them has a 2026 *stabilization* goal (`std::autodiff` is only "maintained" under the 2026 "High-Level ML optimizations" goal, which says it is "not rushing stabilization").
- Do not compress the embedded data, and use no CBOR, bincode or rkyv for it. Superancillary f64 tables are about 90 % of the bytes, and **they compress only 1.15–1.30×** (measured, §2.6).

---

## 1. Summary of recommendations

| # | Decision | Recommendation | Confidence |
|---|---|---|---|
| R1 | Core dependency rule | The default build of published core crates uses **std only**: no third-party crates, no `build.rs`, no proc macros, no `-sys` crates. A CI guard enforces this (§3.4). | High |
| R2 | Opt-in core features | All features are additive and off by default: `rayon` (parallel batch API), `serde` (derives on public data types), `json` (CoolProp-JSON import via `serde_json`) and `libm` (bit-reproducible transcendental math). | High |
| R3 | Edition and MSRV | Edition 2024 everywhere. Develop on the latest stable (1.99.0, 2026-10-01). Core crates declare **`rust-version = "1.85"`** (the edition-2024 floor, Debian 13's rustc and the Linux kernel's minimum). The SIMD backend crate declares 1.89. Raise the MSRV only for a concrete feature, and only to a release at least 6 months old. | Medium |
| R4 | Nightly features | None in published crates: `portable_simd`, `autodiff`, `core_float_math` and `generic_const_exprs` are excluded. Prototype them only in unpublished experiments. | High |
| R5 | Derivatives / AD | (a) Fast path: hand-derived univariate **jets** in τ and δ, combined by outer product. This generalises CoolProp's B-factor recurrence. (b) Generic path: an **in-house `Real` trait and dual / hyper-dual types** shared by f64, SIMD lanes and duals. (c) [`num-dual`](https://crates.io/crates/num-dual) with `default-features = false` as a **dev-dependency oracle** only. (d) No `std::autodiff`. | Medium-High |
| R6 | Embedded data format | A hand-written little-endian binary reader over `include_bytes!` blobs, versioned and checksummed. Small indices (names, aliases, CAS) are generated Rust `static`s. An xtask writes the generated files, which are committed. Floats are emitted as `f64::from_bits` for exactness. No JSON parsing at runtime. | High |
| R7 | Serialization crates | `serde` + `serde_json` only behind the `json` feature and in the datagen xtask. `postcard` only if a serde-based cache format is ever needed. Defer `rkyv` and `zerocopy`. **Ban `bincode`** (RUSTSEC-2025-0141). No CBOR. | High |
| R8 | Compression | None for fluid data. If large generated tables arrive later (TTSE, SBTL), use [`lz4_flex`](https://crates.io/crates/lz4_flex) with `default-features = false, features = ["safe-decode"]` (re-enable `safe-decode`: turning off the defaults switches lz4_flex to its `unsafe` decoder) or [`ruzstd`](https://crates.io/crates/ruzstd) with `default-features = false`, 0 dependencies each, behind a feature. | High |
| R9 | Small dense linear algebra | Hand-written LU with partial pivoting, Cholesky / LDLᵀ and symmetric Jacobi, in two forms: const-generic `[[f64; N]; N]` and slice-based for runtime N. nalgebra and faer are allowed **only in the unpublished datagen** (for example offline companion-matrix eigenvalues). | High |
| R10 | Errors | Hand-written `#[non_exhaustive]` enums implementing `Display` and `core::error::Error`. `thiserror` only in tools. | High |
| R11 | Units | Own `#[repr(transparent)]` SI newtypes in the public API. `uom` interop only in a facade. | Medium-High |
| R12 | Threads | Core types are `Send + Sync` and batches are sequential. The `rayon` feature adds `par_*` batches. Core never spawns threads. `wasm-bindgen-rayon` is browser-facade opt-in only. | High |
| R13 | SIMD | Steps: (1) SoA kernels written to auto-vectorize (no dependencies). (2) A `coolprop-simd` backend crate built on `fearless_simd` 1.0 (0 dependencies, runtime dispatch, AVX2/AVX-512/NEON/simd128). (3) In-house vector `exp`/`ln`. The scalar kernel is always the reference. | Medium |
| R14 | Math backend | Every transcendental call goes through **one internal `math` module**: std by default, `libm` behind a feature, lane versions in the SIMD crate. Integer powers use explicit multiplication, not `powi`. | Medium-High |
| R15 | C ABI | Plain `extern "C"` facade with `#[unsafe(no_mangle)]`. The header is generated by the **cbindgen CLI** in CI and committed (cbindgen is never a build-dependency). Package with `cargo-c`. Do not use safer-ffi or uniffi. | High (C) / Medium (skipping uniffi) |
| R16 | Python | `pyo3` 0.29 + `maturin` 1.15, abi3 wheels. `numpy` is optional in the facade. | High |
| R17 | WASM | Browsers: `wasm-bindgen` (wasm-pack 0.15 or `wasm-bindgen-cli`), with two builds (baseline and `+simd128`). Servers: `wasm32-wasip2` + `wit-bindgen`; re-evaluate `wasm32-wasip3` once it is Tier 2 on stable (Rust 1.100.0, due 2026-11-12). Skip `cargo-component`. Run the core test suite on `wasm32-wasip2` under wasmtime. | Medium-High |
| R18 | Dev tooling | A zero-dependency `verify` dev crate: float comparators, CSV fixtures and a SplitMix64 sampler. `proptest` (std only) for invariants. `criterion` plus `gungraun` in an unpublished bench crate. `insta` is optional, for text snapshots only. | Medium |
| R19 | Supply chain | `cargo-deny` (licences, advisories, bans, sources), a zero-dependency guard, an MSRV job, a weekly latest-dependencies job, `cargo-semver-checks` before release, `cargo-shear`, REUSE 3.3, crates.io Trusted Publishing, and `cargo-about` notices for wheels and WASM bundles. | High |
| R20 | Project licence | `MIT OR Apache-2.0` for code, plus CoolProp's MIT notice for derived data and algorithms. Final decision in D14. | Medium |

---

## 2. Findings by subtopic

### 2.1 Baseline: what CoolProp v8.0.0 pulls in

- **Fetched or vendored dependencies.**
  - `cmake/dependencies.cmake` pins Eigen 5.0.1, msgpack-c, nlohmann_json 3.12.0, Valijson 1.0.6, IF97, REFPROP headers, boost headers, NIST `multicomplex`, fmt 12.0.0 and Catch2 3.8.0 (`cmake/dependencies.cmake:19-131`).
  - `externals/` vendors `incbin` and `miniz-3.1.1`.
- **Derivatives are hand-coded up to 4th order.** The `HelmholtzDerivatives` X-macro bundle is at `include/CoolProp/fluids/Helmholtz.h:62`. Multicomplex-step versions (`one_mcx`, `:303`, `:552`, `:596`; `src/Helmholtz.cpp:295`) exist only to test them.
- **Small linear algebra uses Eigen.**
  - `colPivHouseholderQr` in the N-D Newton solver: `src/Solvers.cpp:69`.
  - A 2×2 solve in transport: `src/Backends/Helmholtz/TransportRoutines.cpp:1168`.
  - Heap-allocated `Eigen::MatrixXd H(N, N)` in the VLE stability code: `src/Backends/Helmholtz/VLERoutines.cpp:2355`, `:2882`.
- **Licence notices.** The v8.0.0 wheel shipped without third-party notices (see [09-data-assets](../coolprop-map/09-data-assets.md) R16). This is a reminder that notices must be generated, not hand-maintained.

### 2.2 Candidate data at a glance

How to read the "Crates L/W/Wasm" and "Closure SLOC" columns:

- **Crates L/W/Wasm** is the number of unique crates in the normal + build dependency closure, excluding the crate itself, for `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` and `wasm32-unknown-unknown`. Default features are used unless noted.
- **Closure SLOC** is the number of non-blank Rust lines in `src/` and `build.rs` of every resolved crate, including the crate itself. It is only a **proxy for compile cost**:
  - `libc` and `linux-raw-sys` are mostly declarations and compile fast per line;
  - generic-heavy crates (nalgebra, typenum, faer) cost more per line;
  - every `syn`-based proc macro serialises the build.

Method in §6.

| Crate (features) | Latest (date) | Licence | rust-version | Crates L/W/Wasm | Closure SLOC | Notes |
|---|---|---|---|---|---|---|
| [num-dual](https://crates.io/crates/num-dual) (default = `nalgebra`) | 0.15.0 (2026-08-12) | MIT OR Apache-2.0 | 1.89 | 19/19/18 | 217k | Default pulls in nalgebra |
| num-dual (`default-features = false`) | same | same | 1.89 | 2/2/2 | 15k | `num-traits` + `autocfg` only; scalar duals only (§2.4) |
| [libm](https://crates.io/crates/libm) | 0.2.16 (2026-01-24) | MIT | 1.63 | 0 | 18k | rust-lang/compiler-builtins |
| [serde](https://crates.io/crates/serde) + derive | 1.0.229 (2026-07-18) | MIT OR Apache-2.0 | 1.56 | 6 | 93k | `syn` chain |
| [serde_json](https://crates.io/crates/serde_json) | 1.0.151 (2026-07-20) | MIT OR Apache-2.0 | 1.71 | 4 | 44k | Dependencies: itoa, memchr, serde_core, zmij |
| [miniserde](https://crates.io/crates/miniserde) | 0.1.46 (2026-07-18) | MIT OR Apache-2.0 | 1.71 | 7 | 64k | Still `syn` |
| [nanoserde](https://crates.io/crates/nanoserde) | 0.2.1 (2025-03-22) | MIT OR Apache-2.0 | 1.81 | 1 | 6.7k | No `syn` |
| [postcard](https://crates.io/crates/postcard) (no default, `alloc`) | 1.1.3 (2025-07-24) | MIT OR Apache-2.0 | – | 10 | 101k | 19 crates with default features |
| [bincode](https://crates.io/crates/bincode) | 3.0.0 (2025-12-16) | MIT | 1.85 | 0 | – | **Tombstone**, see §2.5 |
| [rkyv](https://crates.io/crates/rkyv) | 0.8.18 (2026-08-05) | MIT | 1.81 | 15 | 168k | Zero-copy with validation |
| [ciborium](https://crates.io/crates/ciborium) | 0.2.2 (**2024-01-24**) | Apache-2.0 | 1.58 | 13 | 217k | No release for 2.7 years (repo pushed 2026-09) |
| [minicbor](https://crates.io/crates/minicbor) + derive | 2.3.0 (2026-07-23) | **BlueOak-1.0.0** | – | 5 | 66k | Unusual licence |
| [zerocopy](https://crates.io/crates/zerocopy) + derive | 0.8.59 (2026-09-25) | BSD-2-Clause OR Apache-2.0 OR MIT | 1.56 | 5 | 120k | |
| [bytemuck](https://crates.io/crates/bytemuck) + derive | 1.25.2 (2026-07-19) | Zlib OR Apache-2.0 OR MIT | – | 5 | 65k | |
| [databake](https://crates.io/crates/databake) + derive | 0.2.1 (2026-04-01) | **Unicode-3.0** | 1.82 | 6 | – | ICU4X data baking |
| [phf](https://crates.io/crates/phf) + macros | 0.14.0 (2026-06-21) | MIT | 1.85 | 9 | 62k | Not needed: lookups are rare |
| [miniz_oxide](https://crates.io/crates/miniz_oxide) | 0.9.1 (2026-03-13) | MIT OR Zlib OR Apache-2.0 | – | 1 | 6.9k | DEFLATE/zlib, no unsafe |
| [ruzstd](https://crates.io/crates/ruzstd) (no default) | 0.9.0 (2026-07-26) | MIT | 1.87 | 0 | 11k | 1 crate with default features |
| [lz4_flex](https://crates.io/crates/lz4_flex) (no default) | 0.14.0 (2026-07-14) | MIT | 1.81 | 0 | 4.4k | 1 crate with default features. No default = `unsafe` codec; add `safe-decode` (0 extra crates) |
| [brotli-decompressor](https://crates.io/crates/brotli-decompressor) | 6.0.1 (2026-09-24) | BSD-3-Clause/MIT | – | 2 | – | |
| [nalgebra](https://crates.io/crates/nalgebra) | 0.35.0 (2026-05-24) | **Apache-2.0** | 1.89 | 18/18/17 | 210k | 13 crates / 153k SLOC with `default-features = false, features = ["std"]` |
| [faer](https://crates.io/crates/faer) | 0.24.4 (2026-06-24) | MIT | 1.84 | 73/74/68 | 638k | 44 crates / 443k SLOC with std only |
| [thiserror](https://crates.io/crates/thiserror) | 2.0.21 (2026-09-23) | MIT OR Apache-2.0 | 1.77 | 5 | 61k | `syn` chain |
| [uom](https://crates.io/crates/uom) | 0.38.0 (2026-02-14) | Apache-2.0 OR MIT | 1.68 | 3 | 43k | Built on typenum |
| [rayon](https://crates.io/crates/rayon) | 1.12.0 (2026-04-14) | MIT OR Apache-2.0 | 1.80 | 5 | 43k | |
| [wasm-bindgen-rayon](https://crates.io/crates/wasm-bindgen-rayon) | 1.3.0 (2024-12-21) | Apache-2.0 | – | 26/26/27 | – | Requires nightly (§2.9) |
| [fearless_simd](https://crates.io/crates/fearless_simd) | **1.0.0 (2026-09-21)** | Apache-2.0 OR MIT | 1.89 | **0** | 101k | `libm` optional |
| [wide](https://crates.io/crates/wide) | 1.7.1 (2026-09-14) | Zlib OR Apache-2.0 OR MIT | 1.89 | 2/2/1 | 56k | |
| [pulp](https://crates.io/crates/pulp) | 0.22.3 (2026-06-20) | MIT | – | 12/12/10 | 89k | SIMD layer used by faer |
| [macerator](https://crates.io/crates/macerator) | 0.5.1 (2026-09-28) | MIT OR Apache-2.0 | 1.94 | 22 | – | SIMD layer used by burn |
| [multiversion](https://crates.io/crates/multiversion) | 0.9.0 (2026-09-06) | MIT OR Apache-2.0 | 1.86 | 6 | 61k | Proc macro |
| [wasm-bindgen](https://crates.io/crates/wasm-bindgen) | 0.2.129 (2026-09-25) | MIT OR Apache-2.0 | 1.81 | 11 | 92k | |
| [wit-bindgen](https://crates.io/crates/wit-bindgen) (`macros`) | 0.62.0 (2026-09-10) | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | – | 31–32 | 293k | 0 crates without `macros` |
| [pyo3](https://crates.io/crates/pyo3) | 0.29.3 (2026-09-30) | MIT OR Apache-2.0 | 1.83 | 12 | 270k | 5 crates with `abi3-py310` and no macros |
| [numpy](https://crates.io/crates/numpy) | 0.29.0 (2026-06-13) | BSD-2-Clause | 1.83 | 21 | – | |
| [uniffi](https://crates.io/crates/uniffi) | 0.32.2 (2026-09-23) | **MPL-2.0** | – | 47 | 988k | |
| [safer-ffi](https://crates.io/crates/safer-ffi) | 0.1.13 (**2024-09-17**); 0.2.0-rc1 (2026-01-16) | MIT | 1.72 | 19 | 195k | |
| [cbindgen](https://crates.io/crates/cbindgen) (as a library or build-dependency) | 0.29.4 (2026-06-09) | **MPL-2.0** | 1.74 | 41 | 1.04M | Use the CLI instead |
| [approx](https://crates.io/crates/approx) | 0.5.1 (**2022-01-23**); 0.6.0-rc2 (2026-02-05) | Apache-2.0 | – | 2 | 9k | |
| [float-cmp](https://crates.io/crates/float-cmp) | 0.10.0 (2024-09-20) | MIT | – | 2 | 9k | |
| [proptest](https://crates.io/crates/proptest) | 1.11.0 (2026-03-24) | MIT OR Apache-2.0 | 1.85 | 25/24/22 | 819k | 14 crates with `default-features = false, features = ["std"]` |
| [quickcheck](https://crates.io/crates/quickcheck) | 1.1.0 (2026-02-10) | Unlicense OR MIT | 1.85 | 13 | 309k | |
| [criterion](https://crates.io/crates/criterion) | 0.8.2 (2026-02-04) | Apache-2.0 OR MIT | 1.86 | 50/53/59 | 649k | 42 crates without default features |
| [divan](https://crates.io/crates/divan) | 0.1.21 (**2025-04-10**) | MIT OR Apache-2.0 | 1.80 | 17/15/13 | 780k | Repo active (pushed 2026-07) |
| [gungraun](https://crates.io/crates/gungraun) (formerly iai-callgrind) | 0.20.0 (2026-09-26) | Apache-2.0 OR MIT | 1.88 | 23 | 256k | Valgrind or perf, Linux only |
| [insta](https://crates.io/crates/insta) | 1.49.0 (2026-10-03) | Apache-2.0 | 1.66 | 11/11/6 | 702k | |
| [expect-test](https://crates.io/crates/expect-test) | 1.5.1 (2024-12-21) | MIT OR Apache-2.0 | 1.60 | 2 | 4.9k | |

**Tools.** These are binaries that are installed, never linked:

| Tool | Version (date) |
|---|---|
| [cargo-deny](https://crates.io/crates/cargo-deny) | 0.20.2 (2026-07-09) |
| [cargo-semver-checks](https://crates.io/crates/cargo-semver-checks) | 0.51.0 (2026-10-03) |
| [cargo-msrv](https://crates.io/crates/cargo-msrv) | 0.19.3 (2026-03-25) |
| [cargo-shear](https://crates.io/crates/cargo-shear) | 1.14.0 (2026-09-22) |
| [cargo-machete](https://crates.io/crates/cargo-machete) | 0.9.2 (2026-04-15) |
| [cargo-vet](https://crates.io/crates/cargo-vet) | 0.10.2 (2026-01-13) |
| [cargo-audit](https://crates.io/crates/cargo-audit) | 0.22.2 (2026-06-05) |
| [cargo-about](https://crates.io/crates/cargo-about) | 0.9.2 (2026-08-18) |
| [cargo-auditable](https://crates.io/crates/cargo-auditable) | 0.7.7 (2026-10-02) |
| [cargo-c](https://crates.io/crates/cargo-c) | 0.10.25 (2026-08-25) |
| [maturin](https://crates.io/crates/maturin) | 1.15.0 (2026-08-24) |
| [wasm-pack](https://crates.io/crates/wasm-pack) | 0.15.0 (2026-05-15), now at `github.com/wasm-bindgen/wasm-pack` |
| [wasm-bindgen-cli](https://crates.io/crates/wasm-bindgen-cli) | 0.2.129 |
| [cargo-component](https://crates.io/crates/cargo-component) | 0.21.1 (**2025-03-18**; repo last pushed 2025-07-14 per the [GitHub API](https://api.github.com/repos/bytecodealliance/cargo-component)) |

### 2.3 Edition, toolchain and MSRV

- **Current stable.** Rust **1.99.0, released 2026-10-01** (LLVM 23) ([RELEASES.md](https://github.com/rust-lang/rust/blob/master/RELEASES.md)).
- **Edition 2024** was stabilized in 1.85.0 on 2025-02-20 ([PR 133349](https://github.com/rust-lang/rust/pull/133349)).
  - Its default resolver `"3"` is MSRV-aware.
  - It *prefers* dependency versions whose `rust-version` ≤ ours, with `incompatible-rust-versions = "fallback"`, but it cannot rewrite an explicit requirement ([Cargo resolver docs](https://doc.rust-lang.org/cargo/reference/resolver.html); stabilized in 1.84, [cargo PR 14639](https://github.com/rust-lang/cargo/pull/14639)).
- **Distro anchor.**
  - Debian 13 "trixie" shipped rustc 1.85.0 on 2025-08-09. Trixie now carries 1.85.1 (`1.85.1+dfsg1-1+deb13u1`), and `trixie-backports` offers 1.95 ([sources.debian.org](https://sources.debian.org/src/rustc/)); the anchor is trixie main.
  - The Linux kernel set its minimum Rust to 1.85.0 for that reason ([LKML, April 2026](https://lkml.iu.edu/hypermail/linux/kernel/2604.0/00915.html)).

Stabilizations that matter for this project:

| Version (date) | What | Why it matters here |
|---|---|---|
| 1.63 | `std::thread::scope` ([docs](https://doc.rust-lang.org/std/thread/fn.scope.html)) | Dependency-free fork-join |
| 1.70 / 1.80 | `OnceLock` / `LazyLock` ([OnceLock](https://doc.rust-lang.org/std/sync/struct.OnceLock.html), [LazyLock](https://doc.rust-lang.org/std/sync/struct.LazyLock.html)) | Per-fluid lazy load and global index without `once_cell` or `lazy_static`. `LazyLock::get` arrived in 1.94. |
| 1.81 | `core::error::Error` ([docs](https://doc.rust-lang.org/stable/core/error/index.html)) | Error types that do not need std |
| 1.82 | Float arithmetic in `const fn` ([PR 128596](https://github.com/rust-lang/rust/pull/128596)); `wasm32-wasip2` reaches Tier 2 ([blog](https://blog.rust-lang.org/2024/11/26/wasip2-tier-2/)) | Compile-time constants; WASI target |
| 1.83 | `f64::{from_bits, to_bits, from_le_bytes}` usable in const contexts | Exact generated constants |
| 1.84 | `abs`, `copysign`, `signum` move to `core` ([PR 131304](https://github.com/rust-lang/rust/pull/131304)); MSRV-aware resolver | |
| 1.85 | Edition 2024 | Baseline |
| 1.86 | **Safe `#[target_feature]` functions** (target_feature 1.1, [PR 134090](https://github.com/rust-lang/rust/pull/134090)) | Safe SIMD kernels |
| 1.87 | **Most `std::arch` intrinsics are safe to call** when the caller already has the target feature ([stdarch PR 1714](https://github.com/rust-lang/stdarch/pull/1714)). Loads and stores are still `unsafe`. On wasm32, `bulk-memory` and `nontrapping-fptoint` become default features ([platform doc](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)). | Safe SIMD |
| 1.88 | Let chains (edition 2024) | Ergonomics |
| 1.89 | **AVX-512 target features and intrinsics** ([PR 138940](https://github.com/rust-lang/rust/pull/138940)); standard C ABI for `extern "C"` on `wasm32-unknown-unknown` | x86 SIMD tier; WASM C ABI |
| 1.90 | `lld` is the default linker on x86_64 Linux ([blog](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/)) | Faster links |
| 1.94 | `f64::mul_add` usable in const contexts | |
| 1.95 | **`cfg_select!`** ([docs](https://doc.rust-lang.org/stable/std/macro.cfg_select.html)); `core::hint::cold_path`; `if let` guards | Per-architecture side-by-side code |
| 1.97 | Cargo `build.warnings` config (replaces `-Dwarnings`); v0 symbol mangling becomes the default | CI hygiene |
| 1.98 | **`f64::algebraic_{add,sub,mul,div,rem}`** ([docs](https://doc.rust-lang.org/stable/core/primitive.f64.html#method.algebraic_add)) | Lets LLVM reassociate float sums, so reductions over EOS terms can vectorize |
| 1.99 | Edition-2024 workspace members can override an inherited dependency's `default-features` | Keeps facade feature sets lean |

**Still unstable as of 1.99.** Do not build on these:

- **`std::simd` (`portable_simd`).**
  - The [tracking issue #86656](https://github.com/rust-lang/rust/issues/86656) is open, with no stabilization PR.
  - The September 2026 survey says it is "nightly-only, and still undergoes infrequent breaking API changes", and that doing math on `N` in `Simd<T, N>` is "very incomplete even on nightly" ([State of SIMD in Rust 2026](https://shnatsel.github.io/state-of-simd-rust-2026/)).
- **Float math in `core` (`core_float_math`).**
  - The [tracking issue #137578](https://github.com/rust-lang/rust/issues/137578) is open.
  - It covers only `floor`, `ceil`, `round`, `round_ties_even`, `trunc`, `fract`, `mul_add`, `div_euclid`, `rem_euclid`, `powi`, `sqrt`, `cbrt` and the deprecated `abs_sub`.
  - `exp`, `ln` and `powf` are not even proposed. The blocker is "the quality of our `libm` implementations".
  - A `no_std` core would therefore need the `libm` crate. All three of our targets have std, so this is not needed.
- **`std::autodiff`.**
  - Nightly only, with `RUSTFLAGS="-Zautodiff=Enable"` and `lto = "fat"`.
  - The prebuilt Enzyme rustup component exists only for Linux (x86_64, aarch64 gnu), macOS and Windows **llvm-mingw**. **MSVC and WASM have no prebuilt component**; any other target means building rustc with Enzyme from source ([rustc-dev-guide](https://rust.googlesource.com/rust-lang/rustc-dev-guide/+show/refs/heads/main/src/autodiff/installation.md)).
- **`generic_const_exprs`.** "Full Const Generics" is a 2026 goal ([goal](https://goals.rust-lang.org/2026/const-generics.html)), but its work items are `adt_const_params` (struct/enum const parameters) and `min_generic_const_args` (e.g. `T::ASSOC`), not `generic_const_exprs` arithmetic such as `N + 1`.
  - Do not expect `N + 1` in types soon. Write `Derivs<const N>` as a fixed `[[f64; 5]; 5]`, or avoid `N + 1` in types.
- **No autodiff and no portable-SIMD stabilization goal in 2026.** The 2026 list has no portable-SIMD goal. Its "High-Level ML optimizations" goal only commits to "Continue maintaining std::autodiff" and says "we are not rushing stabilization" ([goal](https://goals.rust-lang.org/2026/high-level-ml.html), [goals](https://goals.rust-lang.org/toc.html)).
- **Float precision is not portable.** The std docs say `exp`, `ln`, `powf` **and `powi`** (also `cbrt`) have "non-deterministic" precision: it "varies by platform, Rust version, and can even differ within the same execution". Among the math functions, `sqrt` and `mul_add` are documented as exactly rounded ([f64 docs](https://doc.rust-lang.org/std/primitive.f64.html)).
  - Linux, Windows and WASM may therefore differ in the last bits. This drives R14.

### 2.4 Automatic differentiation

**What the EOS needs.** From the [02-helmholtz-eos](../coolprop-map/02-helmholtz-eos.md) map:

- 15 αʳ derivatives up to 4th order in (τ, δ);
- α⁰ up to 3rd order;
- 4th mixed derivatives for critical points;
- first and second composition derivatives for mixtures.

CoolProp's GenExp "B-factor" recurrence costs about 1 `exp`, 1 `powInt` and about 60 flops per term for all 15 derivatives. That recurrence is the speed target.

| Option | Dependencies | Orders | Speed | Targets | Status | Fit |
|---|---|---|---|---|---|---|
| Hand-coded analytic (CoolProp) | 0 | Any (hand-written per term) | Fastest | All | About 1.9k hand-written lines in CoolProp. The association term stops at 3rd order (02 map). | Keep only as **univariate jets** for separable terms |
| In-house generic `Real` + dual / hyper-dual / truncated-Taylor `Jet<N>` | 0 | Any, by nesting or a const `N` | Close to hand-coded for univariate jets; nested duals cost more | All, including SIMD lane types | We maintain it (estimated 500–800 lines with tests; unverified) | DRY reference path; one formula for f64, lanes and duals |
| `num-dual` 0.15, `default-features = false` | 2 | Dual, Dual2, Dual3, HyperDual, HyperHyperDual, and nesting ([docs](https://docs.rs/num-dual/latest/num_dual/)) | "can be more costly … compensated" with the right type and caching ([Rehner & Bauer 2021](https://www.frontiersin.org/journals/chemical-engineering/articles/10.3389/fceng.2021.758090/full)) | All | Active (2026-08) | **Dev oracle** |
| `num-dual` with vector duals and gradients | 19 | DualVec, Dual2Vec, HyperDualVec, `gradient`, `hessian`, `jacobian` all need `nalgebra` (checked in `src/lib.rs` of 0.15.0) | – | All | – | Too heavy for core |
| `std::autodiff` (Enzyme) | 0 (toolchain) | Forward and reverse | LLVM-level | **No MSVC, no WASM** | Nightly, experimental | Not viable |
| Multicomplex step (CoolProp's tests) | – | Exact to rounding | Slow | – | Test only | Offline checks only |

- **Evidence that AD is viable for EOS.**
  - teqp computes "all the required thermodynamic properties … without any handwritten derivatives" at "minimal computational overhead" (Bell, Deiters & Leal, Ind. Eng. Chem. Res. 61(17), 2022; [record](https://research-collection.ethz.ch/handle/20.500.11850/549338)).
  - num-dual: "the second criticality condition is calculated with one single evaluation of the Helmholtz energy using the third order dual numbers" (Rehner & Bauer 2021).
- **Why not num-dual in core.**
  - Its `DualNum` trait requires `Primitive: DualNumFloat`, which extends num-traits `Float + FloatConst` and is implemented only for `f32` and `f64` (checked in `src/lib.rs` of 0.15.0).
  - The orphan rule forbids implementing that foreign trait for a foreign SIMD lane type such as `fearless_simd::f64x4`.
  - `Float`'s full method surface (ordering, `integer_decode` and so on) also does not fit vector lanes.
  - The map docs want **one formula over f64, lane types and duals** ([06-eos-families](../coolprop-map/06-eos-families.md) §7, [04-vle-mixtures](../coolprop-map/04-vle-mixtures.md) §7). Only an in-house trait allows that.

### 2.5 Data: parsing, binary formats, zero-copy and codegen

- **The data shape is already known.** See [09-data-assets](../coolprop-map/09-data-assets.md) §4.1 and D3.
  - 136 fluids; 9.19 MiB of minified JSON.
  - About 0.31 MiB of core f64 numbers plus 2.85 MiB of superancillary f64 (2.51 MiB with shared breakpoints).
  - The proposed packed binary is about 3.0–3.3 MiB in total, about 22 KiB per fluid.
  - Runtime JSON parsing is unnecessary. A versioned little-endian reader using `f64::from_le_bytes` decodes 22 KiB in microseconds.
- **Zero-copy.**
  - `include_bytes!` yields `&[u8]` with alignment 1. Reinterpreting it as `&[f64]` needs an `#[repr(C, align(8))]` wrapper plus `bytemuck`, `zerocopy` or `unsafe`.
  - At 22 KiB per fluid, copying into an owned `Box<[f64]>` on first use is simpler, safe, and keeps the "only what is used" memory property.
  - Keep zero-copy (`zerocopy` or `rkyv`) for a later, much larger table format, if it is ever needed.
- **bincode is dead.** [RUSTSEC-2025-0141](https://rustsec.org/advisories/RUSTSEC-2025-0141) says development ceased permanently after a doxxing and harassment incident.
  - 3.0.0 contains only a `compile_error!`.
  - Suggested replacements: wincode, postcard, bitcode, rkyv.
- **CBOR is not needed.**
  - CoolProp's CBOR blob is an implementation detail; the 09 map already proposes to drop it.
  - `ciborium` has made no release since 2024-01.
  - `minicbor`'s BlueOak-1.0.0 licence would need a deny-list exception.
- **serde vs miniserde vs nanoserde vs a hand-written parser** (for the optional user-JSON import only):
  - `serde_json` is correct, ubiquitous and maintained. It preserves integer vs float literals (`Number::is_f64`), which the SA freshness hash needs (09 map §3).
  - Most adopters already compile `serde` and `syn`, so the marginal cost is low for them.
  - `miniserde` still needs `syn`.
  - `nanoserde` is light (1 crate, no `syn`) but less battle-tested.
  - A hand-written JSON parser is not worth maintaining.
- **Codegen to Rust statics vs `include_bytes!`.**
  - Very large array literals are slow to optimise. One 2019 report saw about 24 s in release for a ~53k-element `vec!` literal; `const` / `static` or `include_bytes!` avoid this ([forum](https://users.rust-lang.org/t/long-compile-times-for-a-vector-with-50k-u64-values/35625)). Current rustc may differ (unverified).
  - Rule: generate Rust only for **small indices and enums**, and keep **numeric bulk in blobs**.
- **Where to generate.**
  - Generate in an **xtask, with the output committed**, not in `build.rs`.
  - A `build.rs` would run on every adopter's machine and force them to compile its build-dependencies (serde and others).
  - The crates.io upload limit is 10 MiB (10,485,760 bytes) for the gzip-compressed `.crate` file, with 512 MiB allowed unpacked and per-crate overrides possible ([crates.io `src/config/publish_limits.rs`](https://github.com/rust-lang/crates.io/blob/main/src/config/publish_limits.rs)). The ~3 MiB dataset fits even before the tarball's own gzip.
- **Name lookup.**
  - `phf` adds 9 crates (proc macro) to speed up a lookup that runs once per fluid handle.
  - A generated, sorted `static [(&str, FluidId)]` with binary search costs 0 crates.

### 2.6 Compression of embedded data (measured)

**Setup.** (Fact-check: the 9.19 MiB minified JSON, 2.85 MiB SA f64, gzip 3.70 MiB and SA DEFLATE-9 1.21× / xz-9e 1.30× whole-blob figures were reproduced; the decode speeds, per-fluid ratios and other codecs were not re-run.) CoolProp v8.0.0 `dev/fluids/*.json`, `EOS[0]` only, encoded as a tagged binary: every number as f64, plus strings, keys and 1-byte tags. This layout is larger than the packed format. Reference C codecs on an i7-8700K, single thread. Ratios are given as "whole blob / sum of per-fluid blobs"; per-fluid blobs match lazy, per-fluid access.

| Subset (raw size) | DEFLATE-9 | zstd-19 | LZ4-HC (level 12) | brotli-11 | xz-9e |
|---|---|---|---|---|---|
| Core, non-SA (0.85 MiB; median 6.3 KiB per fluid) | 3.9× / 2.2× | 4.6× / 2.2× | 3.8× / 1.9× | 4.9× / 2.4× | 4.8× / 2.3× |
| **SA raw f64 (2.85 MiB)** | **1.21× / 1.21×** | **1.25× / 1.23×** | **1.17× / 1.15×** | **1.30× / 1.26×** | **1.30× / 1.26×** |
| Everything (4.81 MiB; median 37 KiB per fluid) | 1.62× / 1.61× | 1.83× / 1.65× | 1.59× / 1.48× | 2.02× / 1.78× | 1.96× / 1.75× |
| C decode speed, whole 4.81 MiB blob | 424 MiB/s (11 ms) | ≈1,040–1,300 MB/s (≈4 ms) | ≈5,700 MB/s (<1 ms) | n/m | 66 MiB/s (73 ms) |

- **Per-fluid decode cost.** zstd-19, decoded per fluid in C, has a median of 41 µs (max 55 µs).
- **Pure-Rust decoders.**
  - `lz4_flex` is roughly at C speed: 5,512 vs 5,313 MiB/s decompression in unsafe mode, and 4,540 MiB/s with `safe-decode` ([README](https://github.com/PSeitz/lz4_flex)). The README says the safe codec comes from the *default* features `safe-encode` / `safe-decode`; `default-features = false` selects the unsafe one. It has no HC compressor, but HC output is the standard LZ4 block format, so it decodes it.
  - `ruzstd` is "about 3.5 times slower" than C on very compressible data and about 1.4× slower on less compressible data ([README](https://github.com/KillingSpark/zstd-rs)).
  - `miniz_oxide` is pure Rust "using no unsafe code" ([README](https://github.com/Frommi/miniz_oxide)). Its README gives no speed comparison with zlib.
- **Minified JSON compresses better** (9.19 MiB → gzip 3.69, zstd-19 2.97, brotli 2.79 MiB), but it is still about as large as the uncompressed packed binary.
- **The SA dominates and barely compresses.** Compressing the packed ~3.0–3.3 MiB dataset would save only about 20–30 %. This is my estimate (core about 4×, SA about 1.2×), and it agrees with the 09 map's "only about 20 % gain".
- **Native targets.** The executable image is paged in on demand, so uncompressed fluids that are never touched never become resident. Compression would *add* a heap copy for every fluid that is used.
- **WASM.**
  - Active data segments are **copied into linear memory at instantiation** and then dropped ([WebAssembly spec, instantiation](https://webassembly.github.io/spec/core/exec/modules.html)). Every embedded fluid therefore costs linear memory whether it is used or not.
  - Transfer size is governed by HTTP content-encoding of the `.wasm`. Brotli-11 already gives 2.0× on the whole dataset, so double compression gains nothing.
  - The real WASM levers are **feature-selected fluid subsets** and **runtime-provided per-fluid blobs** (09 map D5 / Q6), not a codec.

### 2.7 Small dense linear algebra

- **Sizes and workloads.**
  - Pure-fluid flashes need 2×2 Newton steps.
  - Mixtures need N×N with N ≤ about 21 (GERG has 21 components): LU or QR solves, an SPD check, and the minimum eigenvalue of a symmetric Hessian (stability, critical points).
  - The companion-matrix eigenvalues for SA extrema move offline into datagen (09 map R1).
  - At N = 20, an LU solve is about N³/3 ≈ 2.7k flops (about 1 µs). The EOS evaluations around it dominate, so the choice of library does not matter for speed.
- **faer.** Its docs say it "is recommended for applications that handle medium to large dense matrices, and its design is not well suited for applications that operate mostly on low dimensional vectors and matrices" ([docs.rs](https://docs.rs/faer/latest/faer/)). Its closure is 73 crates (638k SLOC).
- **nalgebra.** Fixed-size `SMatrix` is a good fit technically, but the cost is 18 crates and 210k SLOC, an Apache-2.0-only licence and MSRV 1.89.
- **Hand-written.**
  - What is needed: partial-pivot LU, Cholesky / LDLᵀ, cyclic Jacobi for symmetric eigenvalues, and Householder QR if needed.
  - About 300–400 lines with tests (estimate).
  - Two forms: const-generic `[[f64; N]; N]` on the stack, and `&mut [f64]` with runtime N reusing a caller scratch buffer.
  - This also fixes CoolProp's per-iteration heap allocation of `Eigen::MatrixXd` (`VLERoutines.cpp:2355`).

### 2.8 Errors and units

- **Errors.**
  - `thiserror` 2 adds the `syn` chain (5 crates, 61k SLOC) to save about 15–30 lines per error enum.
  - Hand-written `#[non_exhaustive]` enums with `impl Display` and `impl core::error::Error` (core since 1.81) keep the Display texts under review and cost 0 crates.
- **Units.**
  - `uom` 0.38 is only 3 crates, but it rests on `typenum` type-level arithmetic.
  - A const-generic port, `tiny-uom`, was written to be a "faster and smaller version" ([docs.rs](https://docs.rs/tiny-uom)). It is abandoned (0.1.0, 2020-11-29).
  - That compile time is uom's main cost was reported only in search summaries (unverified). Measure it with `cargo build --timings` before deciding.
  - The materials study already put units at the facade (D11). Core should expose a handful of SI newtypes (`#[repr(transparent)]`, `const fn new`, `get`) only for the quantities in the API. A facade can add `From<uom::si::f64::…>` conversions.

### 2.9 Parallelism: threads

- **Core.** Models are immutable `Arc` values, so concurrent requests on the same or different fluids need no locks ([01-api-state](../coolprop-map/01-api-state.md) §7).
- **Batch parallelism needs work stealing.** Per-point flash costs vary widely, so a static `std::thread::scope` split balances poorly and pays thread start-up for every batch.
- **rayon 1.12.**
  - 5 crates, MSRV 1.80, maintained ([RELEASES](https://github.com/rayon-rs/rayon/blob/main/RELEASES.md)).
  - Since rayon-core 1.11 (2023-03-03) it "added a fallback when threading is unsupported". On `wasm32-unknown-unknown` it therefore runs on the current thread instead of failing.
  - rayon-core 1.13.0 (2025-08-12) fixed `in_place_scope` for that mode.
- **wasm-bindgen-rayon.** Needs:
  - nightly with `rust-src` (pinned to `nightly-2025-11-15`);
  - `-C target-feature=+atomics,+bulk-memory` plus shared-memory linker flags;
  - COOP/COEP cross-origin isolation for `SharedArrayBuffer` ([README](https://github.com/RReverser/wasm-bindgen-rayon)).
  - Last release 2024-12-21; repo last pushed 2025-11-21.
  - It is an opt-in for one browser-facade build only.
- **WASM targets have no threads by default.** On `wasm32-unknown-unknown`, `std::thread::spawn` panics ([platform doc](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)). The `atomics` target feature (shared memory) is off; atomic types still compile, as plain single-threaded operations (unverified).

### 2.10 SIMD (the user's side-by-side interest)

Sources: [State of SIMD in Rust 2026](https://shnatsel.github.io/state-of-simd-rust-2026/) (2026-09-25; its author discloses that he is now a fearless_simd maintainer and invited the other libraries' authors to review the draft), the [fearless_simd docs](https://docs.rs/fearless_simd/latest/fearless_simd/), [Linebender's SIMD plan](https://linebender.org/blog/a-plan-for-simd/) and the [wide f64x4 docs](https://docs.rs/wide/latest/wide/struct.f64x4.html).

| Option | Crates | Runtime dispatch (needed for generic x86-64 wheels) | Arch coverage | f64 exp / ln | Status |
|---|---|---|---|---|---|
| `std::arch` + `#[target_feature]` + `is_x86_feature_detected!` | 0 | Hand-written: function pointer chosen once | Everything (AVX-512 since 1.89; simd128 at compile time) | Write our own | Stable; safe since 1.86/1.87 except loads and stores |
| **fearless_simd 1.0.0** | **0** | Built in (`dispatch!`), with a scalar fallback | SSE2 baseline, SSE4.2, AVX2, AVX-512 (Ice Lake+), NEON, simd128, relaxed-simd; f64x2/x4/x8 | **None.** The 1.0.0 source has float `sqrt`, `floor`, `mul_add` and similar, but no `exp`, `ln`, `log` or `pow` (checked). The survey: "The biggest gap is trigonometry." | 1.0 with a `SECURITY.md`; survey: "an all-in-one solution where all the parts work together". Safe loads and stores (`from_slice`, `as_slice`, `store_array`) |
| wide 1.7.1 | 2 | **No.** Compile-time features only; the survey calls it "fundamentally incompatible" with multiversioning | x86, NEON, simd128 | Has `exp`, `ln`, `powf_simd`, … with **unspecified precision** | 1.x |
| pulp 0.22.3 | 12 | Yes, but "the most verbose I've ever seen" (survey) | x86-v3/v4, NEON, simd128 | Build it yourself | Used by faer |
| macerator 0.5.1 | 22 | Built in | Broad | Build it yourself | Used by burn; MSRV 1.94 |
| multiversion 0.9.0 | 6 (`syn`) | Per function, with "a little bit of overhead" on each call | Auto-vectorized code | n/a | Fine for big loops, not tiny functions |
| `std::simd` | 0 | No | Everything LLVM supports | Scalar fallbacks | Nightly; breaking changes |

Points that matter for the side-by-side design:

- **Transcendentals are the crux.**
  - αʳ terms need `exp(tᵢ·ln τ + …)`. Across a batch of states, each lane has a different τ, so a vector `exp` is mandatory.
  - No crate offers well-specified f64 `exp` / `ln` (survey).
  - Plan an in-house vector `exp` / `ln`: Cody–Waite reduction plus a minimax polynomial, with error measured against scalar `exp` over the domain. Use integer `d` and `l` exponents, which are all integers in the data (09 map), via repeated multiplication.
- **Distributed binaries.** Python wheels and the C library must run on baseline x86-64, so they need runtime dispatch. `wide` alone cannot provide it.
- **WASM.** Ship two `.wasm` builds, baseline and `+simd128` (simd128 is not on by default), and pick one in JavaScript at load time.
- **Auto-vectorization** is unreliable for large functions (survey). Since 1.98, `algebraic_add` lets sums over terms vectorize. The **scalar reference keeps strict summation order** and is the oracle.
- **FMA pitfall.** `mul_add` is exactly rounded, but it is slow without hardware FMA, and it changes results compared with a separate multiply and add. Use it only inside kernels compiled with `fma` enabled.

### 2.11 FFI and bindings

- **C ABI.**
  - Edition 2024 requires `#[unsafe(no_mangle)]`.
  - Use cbindgen as a **CLI**, then diff and commit the header. As a build-dependency it would add 41 crates (1.04M SLOC, MPL-2.0) to every build.
  - `cargo-c` 0.10.25 builds and installs `.so` / `.dll` / `.a` plus pkg-config files.
  - safer-ffi: its last stable is 0.1.13 (2024-09) and 0.2 is still a release candidate. It adds 19 crates to save little.
- **UniFFI.**
  - MPL-2.0, 47 crates.
  - Officially supports Kotlin, Swift and Python; Ruby support is maintained but gets no new features ([manual](https://mozilla.github.io/uniffi-rs/latest/)).
  - Worth adding only if mobile bindings become a goal.
- **Python.**
  - `pyo3` 0.29.3 (2026-09-30; crates.io `rust-version` 1.83) supports CPython ≥ 3.8, PyPy and GraalPy ([docs.rs](https://docs.rs/pyo3/latest/pyo3/)).
  - It exposes `abi3-py3xx` features and free-threaded `abi3t` / `abi3t-py315` features (crates.io index).
  - `maturin` 1.15.0 is the recommended build tool ([PyO3 guide](https://pyo3.rs/main/)).
  - `numpy` (BSD-2-Clause, 21 crates) is optional, for zero-copy batch arrays.
- **WASM in the browser.**
  - The rustwasm GitHub org was sunset in July–September 2025. `wasm-bindgen` moved to a new `wasm-bindgen` org with new maintainers ([Inside Rust](https://blog.rust-lang.org/inside-rust/2025/07/21/sunsetting-the-rustwasm-github-org)).
  - `wasm-pack` lives on in the same org (0.15.0, 2026-05-15).
  - Pass `Float64Array` for batches. `serde-wasm-bindgen` is not needed (0.6.5, 2024-02, 21 crates).
- **WASM on servers.**
  - `wasm32-wasip2` is Tier 2 and produces components with full std ([platform doc](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip2.html)).
  - WASI 0.3.0, with native async, shipped on 2026-06-11 ([Bytecode Alliance](https://bytecodealliance.org/articles/WASI-0.3)).
  - `wasm32-wasip3` is Tier 3 in stable 1.99, but the promotion to Tier 2 (a 2026 [goal](https://goals.rust-lang.org/2026/wasm-components.html)) has landed: the beta docs list it as Tier 2, "first available on stable in Rust 1.100.0" (due 2026-11-12) ([beta platform doc](https://doc.rust-lang.org/beta/rustc/platform-support/wasm32-wasip3.html)). It adds native async now and cooperative `std::thread` later; neither is needed by a synchronous property kernel.
  - Export through `wit-bindgen` (its `macros` feature adds 31 compile-time crates).
  - `cargo-component` is dormant. Plain `cargo` already targets wasip2 ([wasip2 doc](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip2.html)).

### 2.12 Dev-only tooling

- **Float comparison.**
  - `approx`'s last stable release is from 2022-01 (0.6 has been a release candidate since 2026-02). `float-cmp` was last released 2024-09.
  - The tests map already specifies a zero-dependency `verify` crate with ulp, relative and absolute comparators and tolerance classes ([10-tests-verification](../coolprop-map/10-tests-verification.md) U1).
- **Property testing.**
  - `proptest` 1.11 shrinks failing inputs, which helps when minimising flash failures. With `default-features = false, features = ["std"]` it is 14 crates instead of 25; the defaults pull in `tempfile`, `rusty-fork` and `rustix`.
  - `quickcheck` 1.1 is simpler but gives less control over strategies.
  - Oracle sweeps should use a **seeded, logged, hand-written sampler** (SplitMix64 over (T, ρ) or (T, p) domains). This is reproducible on every target, including wasip2.
- **Benchmarking.**
  - `criterion` 0.8.2 is maintained (by lemmih and berkus; "supports the last three stable minor releases", [repo](https://github.com/criterion-rs/criterion.rs)). It is cross-platform, statistical and heavy (42–59 crates).
  - `divan` is lighter but has made no release since 2025-04.
  - `gungraun` 0.20 (the renamed iai-callgrind, [migration guide](https://gungraun.github.io/gungraun/latest/html/migration/iai-callgrind-to-gungraun.html)) counts instructions and simulates caches under Valgrind or perf. It "cannot be run on Windows" ([repo](https://github.com/gungraun/gungraun)), and it is the right tool for CI regression gates.
  - Keep all benchmarks in an **unpublished** workspace member, so that `cargo test` never builds them.
- **Snapshots.** Numeric results must be compared with tolerances, not text snapshots. `insta` 1.49 (Apache-2.0, 11 crates) or `expect-test` (2 crates) is useful only for error messages, generated headers and `Debug` dumps.

### 2.13 Supply chain and OSS hygiene

- **cargo-deny** checks four things ([book](https://embarkstudios.github.io/cargo-deny/)):
  - advisories (RustSec, including unmaintained and yanked crates);
  - bans (specific crates, duplicate versions, wildcards, features);
  - licences (SPDX allow-list and clarifications);
  - sources (registries and git).
- **cargo-semver-checks** 0.51 lints API changes using rustdoc JSON.
  - It works best on stable; nightly support is best-effort.
  - A GitHub Action exists.
  - Merging it into Cargo is a stated long-term aim ([repo](https://github.com/obi1kenobi/cargo-semver-checks)).
- **Unused dependencies.**
  - `cargo-shear` parses code with `ra_ap_syntax` and has a `--fix` option.
  - `cargo-machete` uses regex and "does not detect all usages" ([cargo-shear README](https://github.com/Boshen/cargo-shear)).
- **MSRV.**
  - A CI job on the pinned old toolchain (`cargo +1.85 check --lib -p <core crates>`) is enough.
  - `cargo-msrv` 0.19 helps find the floor, but it is optional.
- **Lockfiles.** Cargo now says "do what is best for their project". It recommends regular testing against the latest dependencies (scheduled `cargo update`) and lockfiles to validate MSRV ([Rust blog 2023-08-29](https://blog.rust-lang.org/2023/08/29/committing-lockfiles/)).
- **REUSE 3.3** (2024-11-14) ([spec](https://reuse.software/spec-3.3/)):
  - licence texts go in `LICENSES/<SPDX-id>.txt`;
  - files carry `SPDX-License-Identifier` and `SPDX-FileCopyrightText` headers;
  - files that cannot take comments use a `.license` sidecar or `REUSE.toml` annotations with `precedence`. Use this for JSON fixtures and binary blobs.
- **Trusted Publishing on crates.io.**
  - GitHub Actions uses OIDC to request short-lived tokens, so no secrets are stored ([crates.io update 2025-07](https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07); [RFC 3691](https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html)).
  - GitLab CI was added later ([update 2026-01](https://blog.rust-lang.org/2026/01/21/crates-io-development-update)).
  - The first release must be published manually.
- **Third-party notices.** Use `cargo-about` 0.9 for binary artefacts (wheels, `.wasm`, C libraries). This closes the gap that CoolProp's wheel had.

---

## 3. Recommendations for coolprop-rs

### 3.1 Dependency policy (tiers)

Crate names below are placeholders. The tiers are the decision.

| Tier | Covers | Allowed third-party crates | Rule and justification | Confidence |
|---|---|---|---|---|
| **T0 Core, default build** | Data reader, registry, EOS models and kernels, AD types, linalg, solvers, flash | **None** (std only) | No proc macros, `build.rs` or `-sys` crates. `#![forbid(unsafe_code)]`. Adopters compile only our code. Each need is solvable in a few hundred lines (§2.4–2.8). CI enforces it. | High |
| **T1 Opt-in features of core** (additive, off by default) | `rayon` → `par_*` batches. `serde` → derives on public data types. `json` → CoolProp-JSON import/export. `libm` → bit-reproducible math. | rayon; serde (+derive); serde_json; libm | Each adds ≤ about 11 crates. All are rust-lang or dtolnay/serde-rs or rayon-rs maintained, permissively licensed and WASM-clean. | High |
| **T1b SIMD backend crate** (optional dependency of core via a `simd` feature) | Explicit-SIMD kernels for each architecture, side by side with the scalar ones | fearless_simd (0 dependencies) | Its own `rust-version = "1.89"`. Start with `#![forbid(unsafe_code)]` too: fearless_simd offers safe loads and stores (`from_slice`, `store_array`). Allow `unsafe` only for raw `std::arch` paths that need it, each with a `SAFETY` comment. Kernels are tested lane-for-lane against scalar. | Medium |
| **T2 Facades** (separate crates) | C ABI, Python, browser WASM, WASI component, units interop | pyo3, maturin (tool), numpy (optional), wasm-bindgen, js-sys only if needed, wit-bindgen (`macros`), uom | Nothing from a facade leaks into core. Each facade documents its own MSRV. MPL-2.0 is allowed here only by explicit exception (uniffi, if ever). | High |
| **T3 Dev-only** (`[dev-dependencies]` and unpublished crates) | Tests, fixtures, oracles, benchmarks | num-dual (no default), proptest (std only), criterion, gungraun (Linux CI), insta or expect-test (text only) | Never in public APIs. Benchmarks live in an unpublished crate. The `verify` fixture crate has 0 dependencies. | Medium-High |
| **T4 Tools** (unpublished xtask and CI binaries) | Datagen, validation, notices, packaging | serde, serde_json, thiserror/anyhow, nalgebra or faer (offline eigenproblems); cargo-deny, cargo-semver-checks, cargo-shear, cbindgen CLI, cargo-c, maturin, wasm-pack or wasm-bindgen-cli, reuse, cargo-about, wasmtime (test runner) | Pinned by `Cargo.lock` and checked by cargo-deny. Never shipped. | High |
| **Banned** | — | `bincode` ([RUSTSEC-2025-0141](https://rustsec.org/advisories/RUSTSEC-2025-0141)); `serde_cbor` (unmaintained, [RUSTSEC-2021-0127](https://rustsec.org/advisories/RUSTSEC-2021-0127)); `once_cell` and `lazy_static` as direct dependencies (std has `OnceLock` / `LazyLock`); any C-linking or `-sys` crate in T0–T1 (for example `zstd`, C-backed `flate2`); nightly-only crates or features in published crates (`std::simd`, `autodiff`; `wasm-bindgen-rayon` outside an opt-in facade build); GPL, LGPL and AGPL anywhere; MPL-2.0 in T0–T1 | Unmaintained, superseded, breaks pure-Rust or WASM, or licence risk | High |

**Licence allow-list** for cargo-deny:

- `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`
- `BSD-2-Clause`, `BSD-3-Clause`
- `Zlib`, `ISC`, `Unicode-3.0`
- `MPL-2.0` only by per-crate exception in T2 and T4

### 3.2 Decisions per need

| Need | Pick | Rejected (why) | Confidence |
|---|---|---|---|
| Lazy, shared, immutable data | `std::sync::{OnceLock, LazyLock, Arc}` | once_cell, lazy_static (superseded) | High |
| AD | Hand-derived τ and δ jets plus an in-house `Real` with dual types; num-dual as dev oracle | num-dual in core (orphan rule against SIMD lanes; vector duals need nalgebra); std::autodiff (nightly, no MSVC or WASM) | Medium-High |
| Embedded data | Hand-written LE binary + `include_bytes!` + generated small statics (xtask, committed, `f64::from_bits`) | build.rs codegen (cost for every adopter); bulk array literals (slow compiles); CBOR, bincode, rkyv | High |
| JSON import/export | serde + serde_json behind `json` | miniserde (still `syn`), nanoserde (smaller user base), hand-written parser | High |
| Compression | None | DEFLATE, zstd, lz4: SA is incompressible (1.15–1.30×); WASM memory is set by data segments, transfer by HTTP | High |
| Linear algebra | Hand-written const-generic and slice LU, Cholesky and Jacobi | faer (made for large matrices; 73 crates); nalgebra (18 crates, Apache-2.0 only) | High |
| Errors | Hand-written enums with `core::error::Error` | thiserror (proc-macro chain for a few lines saved) | High |
| Units | Own SI newtypes; uom interop in a facade | uom in core (typenum-based generics leak into every public signature; compile cost unverified, so measure it) | Medium-High |
| Thread parallelism | rayon feature; core sequential and `Send + Sync` | `std::thread::scope` splitting (no stealing, start-up cost per batch) | High |
| SIMD | SoA, then fearless_simd, then in-house vector exp/ln | wide (no runtime dispatch), pulp and macerator (more crates, verbose), std::simd (nightly) | Medium |
| Math | One internal `math` module; `libm` feature for determinism | Calling `f64::exp` all over the code; `powi` for integer exponents in kernels | Medium-High |
| C ABI | `extern "C"` + cbindgen CLI + cargo-c | safer-ffi (0.2 not out), uniffi (no C target, MPL) | High |
| Python | pyo3 + maturin (abi3) | – | High |
| Browser and WASI | wasm-bindgen (+wasm-pack 0.15 or wasm-bindgen-cli); wasip2 + wit-bindgen | cargo-component (dormant), wasm-bindgen-rayon by default (nightly) | Medium-High |
| Float compare, fixtures, sampling | Zero-dependency `verify` crate | approx (stale stable), float-cmp | High |
| Property tests | proptest (std only) for invariants | quickcheck | Medium |
| Benchmarks | criterion (wall clock, all OSes) + gungraun (Linux CI instruction counts) | divan (release cadence stalled), but acceptable if preferred | Medium |

### 3.3 Toolchain policy

- **Edition.** Use `edition = "2024"` in every crate and `resolver = "3"` (the edition default).
- **Toolchain.** Develop and run CI on the latest stable (1.99.0 today). Do not pin `rust-toolchain.toml` in the repository; pin only in the benchmark job, for comparable numbers.
- **MSRV.**
  - T0 and T1 crates: `rust-version = "1.85"`. This is the edition-2024 floor, Debian 13's packaged rustc and the Linux kernel's floor, and core needs nothing newer.
  - The SIMD backend crate uses `1.89`: safe `target_feature` and intrinsics, AVX-512, and fearless_simd's own floor.
  - Facades follow their binding crate (pyo3 1.83, wasm-bindgen 1.81).
- **Raising the MSRV.** Allowed only for a concrete stabilization, to a release at least 6 months old, recorded in the CHANGELOG. Use a minor bump before 1.0 and after.
  - Watch-list: let chains (1.88), `cfg_select!` (1.95), `algebraic_*` (1.98).

### 3.4 CI and release hygiene checklist

1. **Zero-dependency guard.** `cargo tree -p <core crate> -e normal,build --depth 1 --prefix none` must print only the crate itself. A tiny xtask check fails the build otherwise.
2. **Matrix.**
   - `cargo test` on x86_64 Linux and x86_64 Windows MSVC.
   - **`cargo test --target wasm32-wasip2`** with wasmtime as the runner. The whole oracle suite then also runs on WASM.
   - `cargo build --target wasm32-unknown-unknown` for the browser facade, in both baseline and `+simd128` variants.
3. **MSRV job.** `cargo +1.85 check --lib` for T0 and T1 crates, and `+1.89` for the SIMD crate.
4. **Latest-dependencies job, weekly.** `cargo update`, then test.
5. **`cargo deny check`** on every PR and daily for advisories.
   - Bans: `bincode`, `serde_cbor`; `once_cell` and `lazy_static` with `wrappers` exceptions for facade dependencies that pull them in (`wasm-bindgen` and `pyo3` depend on `once_cell`).
   - Allow-list as in §3.1. `wildcards = "deny"`. Unknown registries and git sources denied.
   - Check exact key names against cargo-deny 0.20; this is a sketch.
6. **Lints and checks.** Workspace `[lints]` with `unsafe_code = "forbid"` (an override for the SIMD crate only if raw `std::arch` paths need it) and `missing_docs = "warn"`. Run `cargo-shear`. Use Cargo `build.warnings` (1.97+) instead of `-Dwarnings`.
7. **Before each release.** `cargo semver-checks`; `reuse lint`; `cargo about generate` for the wheel, WASM and C bundles.
8. **Publishing.** Use crates.io Trusted Publishing from GitHub Actions; the first release is manual. Commit `Cargo.lock`.
9. **Benchmarks.** gungraun instruction counts on Linux PRs, which fail on a configured regression. criterion runs on demand.

### 3.5 SIMD path, step by step (answers the user's request)

1. **Layout first (no dependencies).** SoA term families with integer `d` and `l`. Superancillary coefficients interleaved `[piece][13][4]` (09 map §7). Batch APIs over `&[f64]` slices. This is the precondition for every later step.
2. **Scalar reference kernels** with strict summation order. They are the oracle and never change behaviour.
3. **Auto-vectorizable variants** in the core crate, using fixed-width chunks (`[f64; 4]` lanes) and `algebraic_add` behind a 1.98-gated feature. Inspect the generated code. Accept a variant only if it gives at least about 1.5× on x86-64-v3 for a batch of ≥ 64 states (target; unverified).
4. **Explicit SIMD backend crate** on fearless_simd. One module per kernel; dispatch once per batch.
   - Native: runtime dispatch.
   - WASM: a compile-time `simd128` build.
   - Start with the two best candidates from the maps: the SA Clenshaw evaluation (no transcendentals, fixed trip count) and the αʳ power-term family (needs vector `exp`).
5. **In-house vector `exp` / `ln`**, accepted when the maximum error is ≤ 2 ulp against `math::exp` across the domain used by the kernels (target).
6. **Equivalence tests** for every SIMD kernel, against scalar, lane by lane, within a few ulp. Branchy solvers (flash, VLE, continuation) stay scalar per state and are parallelised across states with rayon. This is where "some will not suit it" applies.

---

## 4. Warnings

- **Cross-platform bitwise equality is not available from std.** `exp`, `ln`, `powf` and even `powi` have documented non-deterministic precision. Oracle tolerances must allow ULP-level differences between Linux, Windows and WASM.
  - Use the `libm` feature where bit-identical output matters, for example cached tables or golden binaries. That it really gives identical results across targets is unverified; prove it with a cross-target test.
- **The SIMD transcendental gap is real.** None of fearless_simd (checked in the 1.0.0 source), pulp or macerator ships f64 `exp` / `ln` (pulp and macerator per the survey's table). `wide`'s versions have unspecified precision. The `sleef` Rust port is "only a little buggy" (survey).
- **fearless_simd 1.0 is two weeks old** (2026-09-21). Re-check its issue tracker before relying on it. `wide` cannot do runtime dispatch, so it is unsuitable for distributed wheels.
- **WASM memory.** All embedded fluids occupy linear memory in every instance: data segments are copied at instantiation. Do not ship "all 136 fluids" as the only browser build.
- **WASM threads** need nightly, build-std and COOP/COEP. On `wasm32-unknown-unknown`, `std::thread::spawn` panics. Any core code that spawns threads breaks browsers.
- **MSRV-aware resolution only *prefers* compatible versions.** Dev-dependencies such as num-dual 0.15 (1.89), criterion 0.8 (1.86) and gungraun (1.88) need a newer toolchain than core's 1.85. The MSRV job must run `check --lib`, not tests.
- **Feature unification.** In one build, a facade that enables `serde` or `rayon` enables it for every user of core. Keep features strictly additive and never change numeric behaviour.
  - The `libm` feature is the exception that does change bits. Document it, or make it a runtime choice in the `math` module.
- **Maintenance flags.**
  - `bincode` is dead.
  - `ciborium`'s last release is 2024-01.
  - `approx`'s last stable is 2022-01.
  - `divan`'s last release is 2025-04.
  - `cargo-component` is dormant.
  - `wasm-bindgen-rayon`'s last release is 2024-12.
  - `safer-ffi` 0.2 is still a release candidate.
  - `serde-wasm-bindgen`'s last release is 2024-02.
- **Licences to watch if anyone proposes them.**
  - `nalgebra`, `approx`, `ciborium` and `insta` are Apache-2.0 only.
  - `uniffi` and `cbindgen` (as a library) are MPL-2.0.
  - `minicbor` is BlueOak-1.0.0.
  - `databake` is Unicode-3.0.
- **The counts and the SLOC are approximations.** They come from my resolver over the crates.io index (§6). Confirm with `cargo tree -e normal,build --target <triple>` once a toolchain is installed.
- **The compression figures use a tagged layout.** The packed format will have smaller absolute sizes, but the ratios for the SA part should carry over.

## 5. Open questions

1. **MSRV level.** Should core stay at 1.85 (Debian 13 / kernel parity), or use one workspace-wide 1.89? The single value is simpler, enables safe SIMD everywhere and matches nalgebra, num-dual, wide and fearless_simd.
   - The answer depends on whether distro packaging of coolprop-rs is a goal.
2. **Bit-reproducibility.** Is it a product requirement across Linux, Windows and WASM? If yes, `libm` becomes the default math backend and a speed cost must be measured. If no, it stays opt-in.
3. **Shape of the in-house AD.** Should it be a tiny published crate (`coolprop-ad`) that other projects can reuse, or a private module? And which form wins in benchmarks against CoolProp's recurrence: truncated-Taylor `Jet<N>` or nested duals?
4. **SIMD acceptance bar.** Is the bar in §3.5 (≥ about 1.5× per batch, ≤ 2 ulp) right? Which kernel goes first: SA Clenshaw or αʳ power terms?
5. **WASM delivery.** A curated embedded subset, runtime-fetched per-fluid blobs, or both? This is shared with 09 map Q6.
6. **Python packaging.** abi3-only wheels, or also free-threaded (`abi3t` / 3.14t) wheels from day one? Is `numpy` a required dependency of the Python facade?
7. **Mobile bindings.** Are they ever in scope? That is the only case that justifies UniFFI (MPL-2.0, 47 crates).
8. **cargo-vet.** Should it be adopted once there are external contributors, in addition to cargo-deny?
9. **Licence.** Should D14 confirm `MIT OR Apache-2.0` for code plus the CoolProp MIT notice for data? Should generated blobs be tagged `MIT` with CoolProp's copyright line in `REUSE.toml`?

## 6. Method and reproducibility

- **Crate metadata.** Fetched from `https://crates.io/api/v1/crates/<name>` on 2026-10-04: maximum stable version, its publish date, licence, declared `rust-version` and repository. Repository activity (last push, archived) came from `https://api.github.com/repos/<owner>/<repo>`.
- **Dependency counts.** A small resolver over the sparse index (`https://index.crates.io/…`):
  - picks the newest non-yanked, semver-compatible version of each dependency;
  - unifies features (`dep:`, `crate/feature`, `crate?/feature`, implicit optional-dependency features);
  - evaluates `cfg(...)` for `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` and `wasm32-unknown-unknown`;
  - follows normal and build dependencies; dev-dependencies are excluded.
  - Counts are unique crate names, excluding the root.
  - Version-only `cfg(any())` pins (as used by serde_json) are correctly excluded.
- **Closure SLOC.** For each resolved crate version: downloaded from `static.crates.io`, counting non-blank lines in `src/**/*.rs` and `build.rs`. Tests, benches and examples are excluded.
- **Compression.**
  - All 136 `reference/CoolProp/dev/fluids/*.json` files, `EOS[0]` only, SA split out. Encoding: f64 LE for every number, `u32` length plus UTF-8 for strings, sorted keys and 1-byte tags.
  - Codecs: gzip 9, zstd 19 and 3, lz4 12, brotli 11 and xz 9e via their CLIs.
  - In-process decode timing: Python 3.14 `zlib`, `compression.zstd` and `lzma` (median of 20 runs), plus the `lz4 -b` and `zstd -b` built-in benchmarks. Hardware: i7-8700K, Linux 7.2.
- **Scripts.** They lived in the session scratchpad and are not committed. Re-derive them from this description, or replace them with `cargo tree` and `cargo build --timings` once a toolchain is available.

## Verification log

**Date:** 2026-10-04. An adversarial fact-check against primary sources: the crates.io API and sparse index, `static.crates.io` sources, GitHub repos and API, Rust `RELEASES.md`, the rustc platform-support docs (stable, beta, master), the rustc-dev-guide, goals.rust-lang.org, the RustSec advisory-db, the crates.io server source, sources.debian.org, LKML, and the cited papers and blog posts.

**Claims checked: about 190.**

- All 64 crate and tool versions, publish dates, licences and `rust-version` values in §2.2 match crates.io.
- Every Rust stabilization in the §2.3 table matches `RELEASES.md` (1.63–1.99), including 1.99.0 on 2026-10-01 with LLVM 23.
- About 45 dependency counts were re-derived with an independent resolver over the sparse index. All match, including the feature variants: num-dual 19/19/18 and 2; nalgebra 18/18/17 and 13; faer 73/74/68 and 44; proptest 25/24/22 and 14; criterion 50/53/59 and 42; pyo3 12 and 5; wit-bindgen 32 and 0; postcard 10 and 19; and others.
- Also confirmed:
  - the CoolProp citations at `ae81610e` (`dependencies.cmake`, `Helmholtz.h:62`, `one_mcx`, which is only used under `ENABLE_CATCH`, `Solvers.cpp:69`, `TransportRoutines.cpp:1168`, `VLERoutines.cpp:2355/2882`);
  - RUSTSEC-2025-0141 and RUSTSEC-2021-0127, and that bincode 3.0.0 contains only a `compile_error!`;
  - tracking issues #86656 and #137578 (open);
  - rayon's `RELEASES.md` entries;
  - the wasm-bindgen-rayon README (nightly-2025-11-15, flags, COOP/COEP);
  - the quotes from the lz4_flex, ruzstd, miniz_oxide, faer, tiny-uom, cargo-shear, cargo-semver-checks, criterion, gungraun and UniFFI sources, and from Rehner & Bauer 2021;
  - the teqp citation (IECR 61(17), 6010–6028);
  - Trusted Publishing: GitHub in 2025-07, GitLab in 2026-01, and new crates must still be published manually (per the crates.io source);
  - REUSE 3.3 dated 2024-11-14; WASI 0.3.0 on 2026-06-11; the LKML 1.85 bump (2026-04-01);
  - the repository push dates.

**Corrections made:**

1. The crates.io size limit was cited to an unrelated forum thread. It now cites the crates.io source and says that the 10 MiB limit applies to the compressed `.crate` file (512 MiB unpacked).
2. The gungraun rename was cited to rust-lang/rust PR 150080, which is a "compiler-builtins subtree update". It now cites gungraun's migration guide.
3. `lz4_flex` with `default-features = false` selects the **unsafe** codec. R8 and the §2.2 note now add `features = ["safe-decode"]`.
4. `wasm32-wasip3`: "still Tier 3" is stale. It is Tier 3 in 1.99, but it is Tier 2 from 1.100.0 (beta docs). R17 now says to re-evaluate it.
5. `wasm32-unknown-unknown` "has no atomics" is wrong. The platform doc says only that `thread::spawn` panics; the `atomics` (shared-memory) feature is simply off.
6. The survey quotes were corrected:
   - std::simd is "nightly-only, and still undergoes infrequent breaking API changes", not "nightly-only and incomplete";
   - fearless_simd is "an all-in-one solution where all the parts work together", not "most complete all-in-one";
   - pulp's multiversioning is "the most verbose I've ever seen".
   The doc now also discloses that the survey author is a fearless_simd maintainer.
7. fearless_simd's missing `exp`/`ln` is now **confirmed** from the 1.0.0 source, no longer just "verify". It also offers safe loads and stores, so T1b and the CI lint no longer pre-authorise `unsafe` for loads and stores.
8. "Full Const Generics" (2026) targets `adt_const_params` and `min_generic_const_args`, not `generic_const_exprs`.
9. The 2026 goals do include "High-Level ML optimizations", which maintains `std::autodiff` without targeting stabilization. The wording is fixed in the bottom line and §2.3; the conclusion is unchanged.
10. autodiff: "MSVC is not supported" became "no prebuilt component for MSVC or WASM (build from source)".
11. Smaller fixes:
    - `core_float_math` also lists `round_ties_even` and `abs_sub`;
    - `cbrt` is also non-deterministic;
    - the num-dual trait bound is now stated precisely (`DualNumFloat: Float + FloatConst`, implemented for f32 and f64 only);
    - Debian trixie now ships 1.85.1, and backports carry 1.95;
    - the forum compile-time report is dated 2019;
    - fearless_simd also has an SSE2 baseline level.

**Recommendation changes:**

- **R8:** add `safe-decode` for lz4_flex.
- **R17:** re-evaluate wasip3 after 1.100.0.
- **T1b and CI item 6:** the SIMD crate starts as `forbid(unsafe_code)`.
- All other recommendations and confidence levels are unchanged.

**Unverified or not re-run:**

- the closure SLOC column;
- the C decode speeds and the per-fluid zstd timing (41 µs);
- the per-fluid compression ratios and the zstd, LZ4 and brotli figures. The minified-JSON size, SA size, gzip size and the SA DEFLATE / xz ratios were reproduced;
- the "about 1.9k hand-written lines" in CoolProp;
- the exact teqp phrases "without any handwritten derivatives" and "minimal computational overhead" (the ETH record returned HTTP 500);
- `std::autodiff`'s `lto = "fat"` requirement, which comes from the dev-guide's from-source section;
- the in-house size estimates (already marked);
- that atomics on `wasm32-unknown-unknown` lower to plain operations (stated from knowledge of the target, not from the doc).
