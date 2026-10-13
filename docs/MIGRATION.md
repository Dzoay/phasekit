# Migrating from CoolProp

Where each CoolProp call goes in phasekit is the table of [ARCHITECTURE.md §12](ARCHITECTURE.md#12-facades-and-migration).
This page shows the same call in each of phasekit's interfaces, the C shim for hosts that keep CoolProp's
`CoolPropLib.h`, and where phasekit's numbers differ from CoolProp 8.0.0's.

## One call, four ways

CoolProp's `PropsSI("T", "P", 101325, "Q", 0, "Water")`, Water's normal boiling point, in Rust: through the strings
(`phasekit-compat`), then through the typed API, whose state answers any property. This is
`crates/phasekit-compat/examples/migration.rs`; `cargo run -p phasekit-compat --example migration` runs it.

```rust
let t = phasekit_compat::props_si("T", "P", 101_325.0, "Q", 0.0, "Water")?;

let water = Registry::embedded()?.get("Water")?;
let state = water.flash(Input::pq(Pressure::new(101_325.0)?, Quality::new(0.0)?), &FlashOptions::new())?;
assert_eq!(state.t(), t);
```

In C, through `pk_*` (`include/phasekit.h`): every string with its length, a status returned, the value through a
pointer. `crates/phasekit-capi/tests/c/smoke.c` is a complete program, built and run on Linux and Windows.

```c
pk_registry registry = 0;
double t = 0.0;
if (pk_registry_new(PK_DATASET_CORRECTED, &registry) == PK_OK &&
    pk_props_si(registry, "T", 1, "P", 1, 101325.0, "Q", 1, 0.0, "Water", 5, &t) == PK_OK) {
    /* t is 373.124... K */
}
pk_registry_free(registry);
```

In a browser, through the `phasekit` package; free the handle objects with `free()`. Its tests are
`crates/phasekit-wasm/tests/node.rs`.

```js
const registry = new Registry();
const t = registry.PropsSI("T", "P", 101325, "Q", 0, "Water");
registry.free();
```

## Where phasekit differs from CoolProp 8.0.0

- **The grammar is strict** (map 01 R7, R17-R20): no output is echoed from an input, a key is its name or its
  all-uppercase form only, a fluid string is one name (`HEOS::` optional), and a derivative string has nothing around
  it. CoolProp's lax forms are unknown keys here.
- **Numbers.** Each deliberate difference from CoolProp 8.0.0's values (a corrected coefficient, another solver
  branch, a property CoolProp gets wrong) is an entry of the divergence register,
  [`crates/phasekit-verify/src/register.rs`](../crates/phasekit-verify/src/register.rs), with its evidence and the
  test that proves it ([VERIFICATION.md §6](VERIFICATION.md#6-divergence-register)). `DataSet::Parity` reproduces
  CoolProp's data without the corrections.
- **CoolProp's problems designed out**, each with its test: [ROT-REGISTER.md](ROT-REGISTER.md).

## The CoolPropLib.h shim

The Tier A shim is a build of `phasekit-capi` with the `coolproplib-shim` feature (D11). It is distributed as
`libphasekit_coolproplib.so`, `phasekit_coolproplib.dll` or `libphasekit_coolproplib.dylib` (user decision P1), never
under CoolProp's file names. Point the host at that file, or link or rename it locally:

```sh
cargo build --release -p phasekit-capi --features coolproplib-shim --target-dir target/shim
cp target/shim/release/libphasekit_capi.so libphasekit_coolproplib.so
ln -s libphasekit_coolproplib.so libCoolProp.so   # only for a host that loads CoolProp by name
```

It exports these, with CoolProp's signatures, and nothing else (`cargo xtask gates abi`): `PropsSI`, `Props1SI`,
`PhaseSI`, `HAPropsSI`, `get_global_param_string`, `get_fluid_param_string`, `get_param_index`, `get_input_pair_index`,
`set_config_string`, `set_config_double` and `set_config_bool`. The `AbstractState_*` functions (Tier B) are not there.
The default `pk_*` library exports none of these names, so it can share a process with a real CoolProp.

### As in CoolProp 8.0.0

- **Integer codes.** `get_param_index`, `get_input_pair_index` and the `Phase` output have v8.0.0's numbers. A pair
  name that CoolProp's table repeats is its first entry's: `QS_INPUTS` is `QSmolar_INPUTS`, `HQ_INPUTS` is
  `HmolarQ_INPUTS`. An unknown name is -1.
- **`errstring`.** One slot for the process. The last failure on any thread sets it, and
  `get_global_param_string("errstring", ...)` reads and clears it on any thread. A success leaves it as it was.
- **Failures.** `PropsSI`, `Props1SI` and `HAPropsSI` return `HUGE_VAL` (+inf). `PhaseSI` returns 1 and writes
  `unknown: <message>` for a state that fails. `Props1SI` takes the fluid and the output in either order.
- **Buffers.** A buffer of `n` bytes takes a string shorter than `n`. Otherwise the call returns 0, writes nothing and
  sets the `errstring`.
- **FP environment.** Every call masks every FP trap and rounds to nearest inside, then gives back the host's traps and
  rounding and clears the sticky flags. A host that unmasks traps (Delphi) does not fault, and one that polls the flags
  (Excel/VBA) sees none. The `pk_*` library gives the host its own flags back instead.

### Different

- **`HAPropsSI`** is a stub until humid air is added: it returns +inf and sets the `errstring` to "HAPropsSI is not
  available in this version of phasekit". It exists so that hosts that bind every symbol at load time still load.
- **Strings** follow phasekit's strict grammar: one fluid, `HEOS::` optional; no mixtures, fractions or other backends.
  Messages are phasekit's, not CoolProp's.
- **`get_global_param_string`** has `version` (phasekit's), `errstring`, `warnstring` (always empty), `FluidsList` (also
  `fluids_list`, `fluidslist`), `parameter_list` (the 85 output names and their 24 aliases, comma-separated), and
  `incompressible_list_pure`, `incompressible_list_solution`, `mixture_binary_pairs_list` and `predefined_mixtures`,
  which are empty. Every other parameter is refused.
- **`get_fluid_param_string`** has `name`, `aliases` and `BibTeX-EOS`. `ASHRAE34` is refused for every fluid: phasekit's
  data carry no safety class, so no fluid gets CoolProp's "UNKNOWN" or "?". Every other parameter is refused.

### `set_config_*`

The shim keeps its own process-wide settings (user decision P6). The two keys with a phasekit equivalent take effect on
every later shim call. Every other CoolProp 8.0.0 key is accepted with its own type and ignored. An unknown key, or a
value of another type, is refused with the `errstring` set, as v8.0.0 refuses it. The integer keys have no Tier A
setter, so every call that sets one is refused.

| Key | Type | In the shim |
|---|---|---|
| `DONT_CHECK_PROPERTY_LIMITS` | bool | `DomainPolicy::Extrapolate` while true |
| `LIST_STRING_DELIMITER` | string | the delimiter of `FluidsList` and `aliases` |
| `ALLOW_SVDSBTL_IN_PROPSSI` | bool | ignored |
| `ALTERNATIVE_REFPROP_HMX_BNC_PATH` | string | ignored |
| `ALTERNATIVE_REFPROP_LIBRARY_PATH` | string | ignored |
| `ALTERNATIVE_REFPROP_PATH` | string | ignored |
| `ALTERNATIVE_SVDTABLES_DIRECTORY` | string | ignored |
| `ALTERNATIVE_TABLES_DIRECTORY` | string | ignored |
| `ASSUME_CRITICAL_POINT_STABLE` | bool | ignored |
| `CRITICAL_SPLINES_ENABLED` | bool | ignored |
| `CRITICAL_WITHIN_1UK` | bool | ignored |
| `ENABLE_MELTING_CALORIC_HS` | bool | ignored |
| `ENABLE_SUPERANCILLARIES` | bool | ignored |
| `FLOAT_PUNCTUATION` | string | ignored |
| `HENRYS_LAW_TO_GENERATE_VLE_GUESSES` | bool | ignored |
| `HSU_D_TWOPHASE_EOS_POLISH` | bool | ignored |
| `MAXIMUM_TABLE_DIRECTORY_SIZE_IN_GB` | double | ignored |
| `MIXTURE_STABILITY_ALGORITHM` | integer | refused (no Tier A setter) |
| `NORMALIZE_GAS_CONSTANTS` | bool | ignored |
| `OVERWRITE_BINARY_INTERACTION` | bool | ignored |
| `OVERWRITE_DEPARTURE_FUNCTION` | bool | ignored |
| `OVERWRITE_FLUIDS` | bool | ignored |
| `PHASE_ENVELOPE_STARTING_PRESSURE_PA` | double | ignored |
| `REFPROP_DONT_ESTIMATE_INTERACTION_PARAMETERS` | bool | ignored |
| `REFPROP_ERROR_THRESHOLD` | integer | refused (no Tier A setter) |
| `REFPROP_IGNORE_ERROR_ESTIMATED_INTERACTION_PARAMETERS` | bool | ignored |
| `REFPROP_RESOLVE_COOLPROP_ALIASES` | bool | ignored |
| `REFPROP_USE_GERG` | bool | ignored |
| `REFPROP_USE_PENGROBINSON` | bool | ignored |
| `R_U_CODATA` | double | ignored |
| `SAVE_RAW_TABLES` | bool | ignored |
| `SPINODAL_MINIMUM_DELTA` | double | ignored |
| `SVDSBTL_SAMPLING_THREADS` | integer | refused (no Tier A setter) |
| `TABULAR_NX` | integer | refused (no Tier A setter) |
| `TABULAR_NY` | integer | refused (no Tier A setter) |
| `USE_GUESSES_IN_PROPSSI` | bool | ignored |
| `VTPR_ALWAYS_RELOAD_LIBRARY` | bool | ignored |
| `VTPR_UNIFAC_PATH` | string | ignored |
