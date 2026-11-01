# Common tasks for phasekit (user decision JR1). `just` lists them by group; every recipe runs from the repository
# root. The gates are defined in PLAN.md §2.4 (G1-G9) and VERIFICATION.md §11.2 (G8's subcommands): this file runs
# their commands and defines none, so a change to a gate's command changes it here too. CI calls the commands directly
# and needs no `just`.

set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments
set ignore-comments
# Variables are evaluated only by the recipes that use them.
set lazy

# This checkout's shared cargo target directory (user decision SP4), asked for once per `just` run and only by the
# recipes that run cargo, each through `[env("CARGO_TARGET_DIR", target)]`.
target := `scripts/target-dir.sh`
# On disk, not in a RAM-backed /tmp: cargo-mutants copies the tree there and builds each copy.
tmp := env("XDG_CACHE_HOME", env("HOME") / ".cache") / "phasekit-tmp"

[private]
default:
    @just --list --unsorted

# Once per checkout: `fetch`, the site's packages and Chromium, the test counts' merge driver, `toolchain`
[group('setup')]
setup: fetch && toolchain
    cd site && npm ci --no-audit --no-fund
    cd site && npx playwright install chromium
    # CONTRIBUTING.md "Merging the test counts".
    git config merge.phasekit-counts.name "ci/test-counts.txt, per test binary"
    git config merge.phasekit-counts.driver "scripts/merge-counts.sh %O %A %B"

# The pinned, read-only checkouts in reference/: CoolProp v8.0.0, AGA8 and optimized-routines
[group('setup')]
fetch:
    scripts/fetch-coolprop.sh
    scripts/fetch-aga8.sh
    scripts/fetch-optimized-routines.sh

# Check that the tools the gates need are installed (read-only and offline)
[group('setup')]
toolchain:
    scripts/check-toolchain.sh

# Debug build of every crate
[env("CARGO_TARGET_DIR", target)]
[group('build')]
build:
    cargo build --workspace

# The API docs of every crate, intra-doc links checked as in CI; `just doc --open` opens them
[env("CARGO_TARGET_DIR", target)]
[env("RUSTDOCFLAGS", "-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links")]
[group('build')]
doc *args:
    cargo doc --workspace --no-deps "$@"

# The site into site/dist: the browser package's modules, the fluid packs and the notices, then Vite
[env("CARGO_TARGET_DIR", target)]
[group('build')]
site:
    scripts/site/build.sh

# Vite on :5173 with hot reload, over the modules and packs of the last `just site`
[group('run')]
site-dev:
    cd site && npm run dev

# A fluid's data, readable: `just fluid list`, `just fluid show Water`, `just fluid diff`
[env("CARGO_TARGET_DIR", target)]
[group('run')]
fluid *args:
    cargo xtask fluid "$@"

# docs/MIGRATION.md's example: CoolProp's PropsSI call, then the same state through the typed API
[env("CARGO_TARGET_DIR", target)]
[group('run')]
example:
    cargo run -p phasekit-compat --example migration

# Every crate's tests, or those nextest's arguments pick: `just test -p phasekit-core saturation`
[env("CARGO_TARGET_DIR", target)]
[group('test')]
test *args:
    cargo nextest run "$@"

# Every crate's doctests, or those named: `just test-doc -p phasekit-core`
[env("CARGO_TARGET_DIR", target)]
[group('test')]
test-doc *args:
    cargo test --doc "$@"

# The site's Vitest tests, after `just site`; `just test-site units`: the files matching "units"
[group('test')]
test-site *args:
    cd site && npx vitest run "$@"

# Playwright on Chromium against site/dist, after `just site`; `just e2e -g diagram`: those tests
[group('test')]
e2e *args:
    cd site && npx playwright test "$@"

# Format the Rust code
[group('check')]
fmt:
    cargo fmt --all

# The gates that run no tests, changing nothing: G1, G2, G5-G7, deny, shear and reuse
[group('check')]
check: g1 g2 g5 g6 g7 deny shear reuse

# G8's counts runs G3's and G4's tests, so the suite runs once. Mutants and perf have CI jobs of their own.
# Every gate but G8's mutants and perf (`just mutants`, `just perf`), and deny, shear and reuse
[group('check')]
gates: check (g8 "all" "--except" "mutants,perf") g9

# cargo deny: licences, bans, sources and advisories (deny.toml)
[group('check')]
deny:
    cargo deny check

# cargo shear: no unused dependency
[group('check')]
shear:
    cargo shear

# reuse lint: every file's licence and copyright (REUSE.toml), at CI's version
[group('check')]
reuse:
    uvx "reuse==$(sed -n 's/^  REUSE_VERSION: "\(.*\)"$/\1/p' .github/workflows/ci.yml)" lint

# actionlint on .github/workflows, after editing them (no gate runs it)
[group('check')]
workflows:
    actionlint

# G1: formatting
[group('gates')]
g1:
    cargo fmt --all --check

# G2: clippy on every crate and target, warnings denied
[env("CARGO_TARGET_DIR", target)]
[group('gates')]
g2:
    cargo clippy --workspace --all-targets -- -D warnings

# G3: every test on the host, then the doctests
[env("CARGO_TARGET_DIR", target)]
[group('gates')]
g3:
    cargo nextest run --workspace --profile ci
    cargo test --doc --workspace

# G4: the tests on wasm32-wasip2 under wasmtime, xtask aside, then their doctests
[env("CARGO_TARGET_DIR", target)]
[group('gates')]
g4:
    cargo nextest run --workspace --exclude phasekit-xtask --target wasm32-wasip2 --profile ci
    cargo test --doc --workspace --exclude phasekit-xtask --target wasm32-wasip2

# G5: the browser target builds
[env("CARGO_TARGET_DIR", target)]
[group('gates')]
g5:
    cargo check --workspace --target wasm32-unknown-unknown

# G6: the Windows target builds (a check only: no MSVC linker here)
[env("CARGO_TARGET_DIR", target)]
[group('gates')]
g6:
    cargo check --workspace --all-targets --target x86_64-pc-windows-msvc

# G7: clippy on core and compat without default features
[env("CARGO_TARGET_DIR", target)]
[group('gates')]
g7:
    cargo clippy -p phasekit-core -p phasekit-compat --no-default-features --all-targets -- -D warnings

# G8: `cargo xtask gates all`, or those named: `just g8 decisions`, `just g8 all --except perf`
[env("CARGO_TARGET_DIR", target)]
[group('gates')]
g8 *args="all":
    cargo xtask gates "$@"

# G9: the site's build, prettier, svelte-check, Vitest, StrykerJS, its budget and Playwright
[env("CARGO_TARGET_DIR", target)]
[group('gates')]
g9:
    scripts/site/check.sh

# G8's mutants gate on the Rust changed since origin/main; `just mutants --shard 1/4`: CI's first slice
[env("CARGO_TARGET_DIR", target)]
[env("TMPDIR", tmp)]
[group('mutants')]
mutants *args:
    mkdir -p "$TMPDIR"
    cargo xtask gates mutants "$@"

# StrykerJS on the site's TypeScript changed since origin/main, after `just site`; `--all`: every file
[group('mutants')]
mutants-site *args:
    scripts/site/mutants.sh "$@"

# G8's instruction gate (gungraun, Valgrind); `just perf --update` makes this run's counts the baseline
[env("CARGO_TARGET_DIR", target)]
[group('perf')]
perf *args:
    cargo xtask gates perf "$@"

# Record the criterion benches for this branch's step; `just bench scaling`: the thread scaling
[env("CARGO_TARGET_DIR", target)]
[group('perf')]
bench *args:
    cargo xtask bench --record "$@"

# Raise ci/test-counts.txt's minimums to this tree's (`gates counts --update`: G3's and G4's tests)
[env("CARGO_TARGET_DIR", target)]
[group('tools')]
counts:
    cargo xtask gates counts --update

# Regenerate phasekit-data's blobs, index and features from the pinned fluid JSON
[env("CARGO_TARGET_DIR", target)]
[group('tools')]
datagen:
    cargo xtask datagen

# CoolProp 8.0.0 oracle fixtures, given gen.py's arguments (VERIFICATION.md §3.1)
[env("CARGO_TARGET_DIR", target)]
[group('tools')]
oracle *args:
    cargo xtask oracle "$@"
