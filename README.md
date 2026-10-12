# phasekit

A modular, memory-safe Rust library for thermophysical properties, starting with fluids and growing into materials
and all states of matter. It is a from-scratch successor to [CoolProp](https://github.com/CoolProp/CoolProp), not a
binding, verified against CoolProp 8.0.0 and the published literature.

> **Status: in development, not yet released.** The closed milestones are the repository's `m*` tags; the steps and
> what v0.1 holds are in [docs/PLAN.md](docs/PLAN.md).

## Goals

- **Modular and extensible:** new equation-of-state families, mixtures and, later, solids and other states of matter
  plug in without editing the core.
- **Concurrent by construction:** immutable shared models and small `Copy` states, so many parallel requests on the
  same or different fluids need no locks. Fluids load lazily, only when a request needs them.
- **Fast single calls and batches**, with a scalar reference path that any future SIMD or threaded path must match
  bit for bit.
- **Cross-platform:** Linux, Windows and WebAssembly (browser and WASI).
- **Minimal dependencies:** the core uses only `std` until the parallel-batch milestone.
- **Verified:** CoolProp 8.0.0 is the oracle; printed literature arbitrates; every deliberate difference is a cited,
  tested entry in a divergence register.
- **Easy migration:** a `PropsSI`-style string API, a C ABI with a small CoolProp-compatible shim, and later Python.

## v0.1 (planned)

All 136 CoolProp 8.0.0 pure and pseudo-pure fluids; all 19 input pairs; transport properties and surface tension
where models exist; melting lines; reference states; derivative outputs; parallel batches; the `PropsSI`-style API, the
C ABI and a browser WebAssembly package. Cubic models, mixtures, IF97, incompressibles, humid air, Python and plotting
follow afterwards ([PLAN.md §4](docs/PLAN.md#4-post-01-roadmap)).

## Documentation

| Document | What it is |
|---|---|
| [docs/BRIEF.md](docs/BRIEF.md) | The requirements, verbatim, and the decisions the design had to make |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, core types, decisions D1-D17, milestones |
| [docs/PLAN.md](docs/PLAN.md) | The test-first implementation plan, step by step (M0-M10 to v0.1, then the roadmap) |
| [docs/VERIFICATION.md](docs/VERIFICATION.md) | Oracle fixtures, literature arbiters, tolerances, the divergence register, CI gates |
| [docs/ROT-REGISTER.md](docs/ROT-REGISTER.md) | Every CoolProp problem found, how phasekit avoids it, and the test that proves it |
| [docs/DECISIONS.md](docs/DECISIONS.md) | The maintainer's decisions, dated; they override older text |
| [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md) | The dependency tiers |
| `docs/coolprop-map/`, `docs/research/` | Frozen evidence: a 15-part map of CoolProp 8.0.0 and the design-stage research |
| `docs/history/` | Frozen: how the architecture was chosen (four proposals, judges, critiques) and the type sketch |

## Repository layout

```text
docs/        design, plan and evidence (above)
scripts/     fetch-coolprop.sh, check-toolchain.sh, the oracle generator (oracle/) and the C++ baseline (baseline/)
crates/      the Cargo workspace: phasekit-core, -data, -compat, -verify, -xtask; later -capi, -wasm, -py
reference/   gitignored: the pinned CoolProp checkout and local papers
```

## Getting started (contributors)

Requirements: the latest stable Rust with the `wasm32-unknown-unknown`, `wasm32-wasip2` and `x86_64-pc-windows-msvc`
targets, [uv](https://docs.astral.sh/uv/) for the CoolProp oracle, and the cargo tools that
`scripts/check-toolchain.sh` lists; it reports what is missing and how to install it.

```sh
scripts/check-toolchain.sh         # reports which of the tools above are missing
scripts/fetch-coolprop.sh          # pinned, read-only CoolProp v8.0.0 checkout in reference/
cargo nextest run --workspace      # the workspace's tests
cargo test --doc --workspace       # and its doctests
actionlint                         # after editing .github/workflows/
uv run --no-project --python 3.12 --with CoolProp==8.0.0 \
  python -c "import CoolProp.CoolProp as CP; print(CP.PropsSI('H', 'T', 300, 'P', 101325, 'Water'))"
```

Then read [CONTRIBUTING.md](CONTRIBUTING.md). AI coding agents start with [AGENTS.md](AGENTS.md).

## How changes land

Every change is a pull request with a Conventional Commit title, squash-merged once CI is green, so `main` is linear;
versions follow SemVer. The rules: [CONTRIBUTING.md](CONTRIBUTING.md).

## Reporting problems

- Wrong values, errors or crashes: open a **Bug report** issue.
- A published reference value that disagrees: open a **Literature disagrees** issue. Literature beats CoolProp here.
- Security problems: report privately, see [SECURITY.md](SECURITY.md).

## Licence

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

Fluid data and re-implemented algorithms derive from CoolProp (MIT); the saturation-curve coefficients were fitted by
NIST's fastchebpure. See [LICENSE-THIRD-PARTY](LICENSE-THIRD-PARTY). phasekit is not affiliated with or endorsed by
the CoolProp project or NIST.
