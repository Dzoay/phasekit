# phasekit

A modular, memory-safe Rust library for thermophysical properties, starting with fluids and growing into materials
and all states of matter. It is a from-scratch successor to [CoolProp](https://github.com/CoolProp/CoolProp), not a
binding, verified against CoolProp 8.0.0 and the published literature.

> **Status: milestone M0 (toolchain, workspace, gates and CI) is done; M1 (verification kit and oracle) is next.**
> This repository holds the architecture, the step-by-step test-driven plan and the verification design. There is no
> usable library yet; progress follows [docs/PLAN.md](docs/PLAN.md).

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
| [docs/design/04-user-decisions.md](docs/design/04-user-decisions.md) | The maintainer's decisions on the open questions |
| `docs/coolprop-map/`, `docs/research/` | The evidence: a 15-part map of CoolProp 8.0.0 and fact-checked research |
| `docs/design/` | How the architecture was chosen (four proposals, judges, critiques) and the compiled type sketch |

## Repository layout

```text
docs/        design, plan and evidence (above)
scripts/     fetch-coolprop.sh, check-toolchain.sh; later the oracle generator and the C++ baseline harness
crates/      the Cargo workspace: phasekit-core, -data, -verify, -xtask; -compat from M5.9; later -capi, -wasm, -py
reference/   gitignored: the pinned CoolProp checkout and local papers
```

## Getting started (contributors)

Requirements: stable Rust (1.99 or newer; the targets `wasm32-unknown-unknown`, `wasm32-wasip2` and
`x86_64-pc-windows-msvc`), [uv](https://docs.astral.sh/uv/) for the CoolProp oracle, and `wasmtime`, `cargo-deny`,
`cargo-shear` and `cargo-mutants` (`cargo install --locked wasmtime-cli cargo-deny cargo-shear cargo-mutants`).

```sh
scripts/check-toolchain.sh         # reports which of the tools above are missing
scripts/fetch-coolprop.sh          # pinned, read-only CoolProp v8.0.0 checkout in reference/
cargo test --workspace             # the workspace
uv run --no-project --python 3.12 --with CoolProp==8.0.0 \
  python -c "import CoolProp.CoolProp as CP; print(CP.PropsSI('H', 'T', 300, 'P', 101325, 'Water'))"
```

Then read [CONTRIBUTING.md](CONTRIBUTING.md). AI coding agents start with [AGENTS.md](AGENTS.md).

## How changes land

- **Linear history:** every change is a pull request, squash-merged, so `main` has one commit per PR and every commit
  passed CI.
- **[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):** PR titles like
  `feat(core): power-term jets match the oracle`, with a `Plan-Step: M3.2` footer for plan steps.
- **[SemVer](https://semver.org/):** one version for all crates; before 1.0 a breaking change bumps the minor version.
  Releases are tags `vX.Y.Z`, published to crates.io and npm from CI.

Details: [CONTRIBUTING.md](CONTRIBUTING.md).

## Reporting problems

- Wrong values, errors or crashes: open a **Bug report** issue.
- A published reference value that disagrees: open a **Literature disagrees** issue. Literature beats CoolProp here.
- Security problems: report privately, see [SECURITY.md](SECURITY.md).

## Licence

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

Fluid data and re-implemented algorithms derive from CoolProp (MIT); the saturation-curve coefficients were fitted by
NIST's fastchebpure. See [LICENSE-THIRD-PARTY](LICENSE-THIRD-PARTY). phasekit is not affiliated with or endorsed by
the CoolProp project or NIST.
