//! Developer tasks (D7, D13, D17). Never published. Subcommands of the real tool:
//!
//! - `datagen`: pinned v8.0.0 JSON → the one serde mirror (here, not in core: S-12) with a
//!   literal-kind-preserving parse (logged Chlorine duplicate-key waiver) → FNV-1a `source_eos_hash`
//!   recomputation gate → closed enums → `data/corrections.csv` validated against the JSON values it
//!   replaces → precomputed superancillary extrema and inverses → ECS reference graph proved acyclic → one LE
//!   blob per fluid (EOS section written by `phasekit_core::internal::EosRecord::encode`, the same encoder the
//!   runtime hash gate uses, E14), the name/alias/CAS index with declared references, and the per-fluid
//!   features (with ECS implications) in `phasekit-data`.
//! - `oracle`: drives `uv run --no-project --python 3.12 --with CoolProp==8.0.0` with a fresh state per case,
//!   scrubbed `COOLPROP_*`/`PXFLASH_*`, all 38 config keys recorded and `oracle.lock` checked.
//! - `gates`: zero-dependency guard, executed-test-count ≥ manifest, `#[ignore]` ids, fixture manifest drift,
//!   the facade feature-tree check and .wasm size budget (E7), perf-table recording from M3 (E9).

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    let known = ["datagen", "oracle", "gates"];
    if known.contains(&task.as_str()) {
        println!("phasekit-xtask {task}: lands at M1/M2 (core {})", core::any::type_name::<phasekit_core::Registry>());
    } else {
        println!("usage: cargo xtask <{}>", known.join("|"));
    }
}
