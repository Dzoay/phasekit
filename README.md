# phasekit

> [!WARNING]
> **Work in progress, not yet released.** phasekit has not reached its first release: there is no `v0.1.0` tag, and
> nothing is published on crates.io or npm. APIs, data formats and computed values can change in any commit, without
> notice or a deprecation period. Try it, report what you find, but do not depend on it yet.

A modular, memory-safe Rust library for thermophysical properties, starting with fluids and growing into materials
and all states of matter. It is a from-scratch successor to [CoolProp](https://github.com/CoolProp/CoolProp), not a
binding, verified against CoolProp 8.0.0 and the published literature.

## Goals

- **Modular and extensible:** new equation-of-state families, mixtures and, later, solids and other states of matter
  plug in without editing the core.
- **Concurrent by construction:** immutable shared models and small `Copy` states, so many parallel requests on the
  same or different fluids need no locks. Fluids load lazily, only when a request needs them.
- **Fast single calls and batches**, with a scalar reference path that the threaded and SIMD paths match bit for bit.
- **Cross-platform:** Linux, Windows and WebAssembly (browser and WASI).
- **Minimal dependencies:** the core needs only `std`; threads (`rayon`), the portable maths library (`libm`) and SIMD
  lanes are opt-in features.
- **Verified:** CoolProp 8.0.0 is the oracle; printed literature arbitrates; every deliberate difference is a cited,
  tested entry in a divergence register.
- **Easy migration:** a `PropsSI`-style string API, a C ABI with a small CoolProp-compatible shim, and later Python.

## Where it stands

The work follows a test-first plan, one step per pull request. Finished milestones are tagged in git (`m0` onwards;
`git tag` lists them); the open ones, and their steps, are in [docs/PLAN.md](docs/PLAN.md). The workspace's version already reads
0.1.0 and [CHANGELOG.md](CHANGELOG.md) has a 0.1.0 section written ahead of the tag, but `v0.1.0` is tagged only when
the maintainer decides ([DECISIONS.md](docs/DECISIONS.md) RL4), and the crates and the npm package are published after
that.

**On `main` now**, built and tested:

| Area | What works |
|---|---|
| Fluids | All 136 CoolProp 8.0.0 pure and pseudo-pure fluids on their Helmholtz equations of state; van der Waals, SRK and Peng-Robinson cubics of each pure fluid (`phasekit-cubic`) |
| Mixtures | Multi-fluid Helmholtz mixtures with CoolProp 8.0.0's binary pairs and predefined mixtures, and GERG-2008 (`phasekit-mix`): vapour-liquid equilibrium (a PT flash with a stability test, bubble and dew points, any quality), critical points, phase envelopes and all 19 input pairs; CoolProp's mixture strings (`HEOS::R32[0.7]&R125[0.3]`, `R410A.mix`) in `PropsSI`, the C ABI and the browser package |
| Inputs | All 19 input pairs, below the triple point refused unless a call asks to extrapolate |
| Outputs | Thermodynamic properties with first and second partial derivatives, exact virials, transport properties and surface tension where models exist, melting lines, reference states |
| Batches | Point-major batches with a status per cell; parallel (`rayon`) and SIMD-lane (`simd`) execution, bit for bit equal to the scalar path |
| Interfaces | The typed Rust API, CoolProp's `PropsSI` strings, a C ABI (`pk_*`) with a CoolPropLib.h-compatible shim, a browser WebAssembly package |

**In progress:**

- **The site** (M21): a single page at <https://dzoay.github.io/phasekit/>, deployed from `main`, that runs the browser
  package in your browser. The calculator and the diagrams (p–h, T–s, p–T, T–ρ and h–s, with phase regions and a
  state picked under the cursor) are there; sweeps, an in-browser benchmark, all 136 fluids and mixtures follow.

**Later** ([PLAN.md §4](docs/PLAN.md#4-post-01-roadmap)): IAPWS-IF97 and ice, PC-SAFT, Python, incompressibles,
humid air and diagram geometry.

## Trying it

Nothing is published yet, so everything below builds from a checkout of this repository.

| Use it from | Build | Start with |
|---|---|---|
| Rust | a git dependency on `phasekit-core` (the typed API) or `phasekit-compat` (CoolProp's strings) | `crates/phasekit-compat/examples/migration.rs` |
| C | `cargo build --release -p phasekit-capi`: the `pk_*` library, its header `crates/phasekit-capi/include/phasekit.h` | `crates/phasekit-capi/tests/c/smoke.c` |
| A CoolProp host | the same build with `--features coolproplib-shim`: CoolProp's Tier A functions, distributed as `phasekit_coolproplib` | [docs/MIGRATION.md](docs/MIGRATION.md) |
| A browser | `scripts/release/npm-package.sh <version>`: the npm package `phasekit` (`phasekit/simd` for the `simd128` build) | `crates/phasekit-wasm/tests/node.rs` |

## Documentation

| Document | What it is |
|---|---|
| [docs/BRIEF.md](docs/BRIEF.md) | The requirements, verbatim, and the decisions the design had to make |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, core types, decisions D1-D17, milestones |
| [docs/PLAN.md](docs/PLAN.md) | The test-first implementation plan, step by step: M0-M10 for v0.1, then the roadmap from M11 |
| [docs/VERIFICATION.md](docs/VERIFICATION.md) | Oracle fixtures, literature arbiters, tolerances, the divergence register, CI gates |
| [docs/ROT-REGISTER.md](docs/ROT-REGISTER.md) | Every CoolProp problem found, how phasekit avoids it, and the test that proves it |
| [docs/DECISIONS.md](docs/DECISIONS.md) | The maintainer's decisions, dated; they override older text |
| [docs/MIGRATION.md](docs/MIGRATION.md) | From CoolProp to phasekit, in each interface; the C shim; where the numbers differ |
| [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md) | The dependency tiers |
| `docs/coolprop-map/`, `docs/research/` | Frozen evidence: a 15-part map of CoolProp 8.0.0 and the design-stage research |
| `docs/history/` | Frozen: how the architecture was chosen (four proposals, judges, critiques) and the type sketch |

## Repository layout

```text
docs/        design, plan and evidence (above)
crates/      the Cargo workspace: phasekit-core, -data, -compat, -cubic, -mix, -capi, -wasm, -verify, -xtask; -py later
site/        the GitHub Pages site (Svelte, Vite, TypeScript) over the browser package
scripts/     fetch-coolprop.sh, check-toolchain.sh, the oracle generator (oracle/), the C++ baseline (baseline/), the
             release artefacts (release/) and the site's build and checks (site/)
data/        the fluid data's lock, cited corrections and citations
ci/          the gates' recorded test counts and exemption lists
reference/   gitignored: the pinned CoolProp checkout and local papers
```

## Getting started (contributors)

Requirements: the latest stable Rust with the `wasm32-unknown-unknown`, `wasm32-wasip2` and `x86_64-pc-windows-msvc`
targets, [uv](https://docs.astral.sh/uv/) for the CoolProp oracle, Valgrind for the instruction gate, Node.js 24 or
later with npm for the browser package's tests and the site, and the cargo tools that `scripts/check-toolchain.sh`
lists; it reports what is missing and how to install it.

```sh
scripts/check-toolchain.sh         # reports which of the tools above are missing
scripts/fetch-coolprop.sh          # pinned, read-only CoolProp v8.0.0 checkout in reference/
cargo nextest run --workspace      # the workspace's tests
cargo test --doc --workspace       # and its doctests
scripts/site/check.sh              # the site's gate (once: npx playwright install chromium)
actionlint                         # after editing .github/workflows/
uv run --no-project --python 3.12 --with CoolProp==8.0.0 \
  python -c "import CoolProp.CoolProp as CP; print(CP.PropsSI('H', 'T', 300, 'P', 101325, 'Water'))"
```

Every gate a pull request passes, G1-G9, is defined in [PLAN.md §2.4](docs/PLAN.md#24-standard-gates). Then read
[CONTRIBUTING.md](CONTRIBUTING.md). AI coding agents start with [AGENTS.md](AGENTS.md).

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
