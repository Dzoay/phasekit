#!/usr/bin/env python3
"""The CoolProp 8.0.0 oracle fixture generator (docs/VERIFICATION.md section 3.1).

Standard library plus CoolProp only, so the uv environment is exactly the pinned wheel. `cargo xtask oracle` runs it
in a scrubbed environment:

    env -i PATH="$PATH" HOME="$HOME" LC_ALL=C TZ=UTC PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1 \
      uv run --no-project --python 3.12 --with CoolProp==8.0.0 \
      python scripts/oracle/gen.py --lock crates/phasekit-verify/fixtures/oracle.lock --kind facts --set smoke --out DIR

Five start-up assertions run before any row is computed; any failure exits non-zero. A kind is added here in the
PLAN.md step that first uses it.
"""

import argparse
import hashlib
import json
import math
import os
import platform
import struct
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FLUIDS_DIR = ROOT / "reference" / "CoolProp" / "dev" / "fluids"
KINDS = ["facts", "checkpoints", "term", "eos", "crit", "sat", "flash", "transport", "sigma", "melt", "refstate",
         "props", "codes"]
SCRUBBED_PREFIXES = ("COOLPROP_", "PXFLASH_")
FLUID_COUNT = 136

# splitmix64-v1, seed 1: the vector phasekit_verify::sample checks too (VERIFICATION.md section 3.1, assertion 5).
GOLDEN = [0x910A2DEC89025CC1, 0xBEEB8DA1658EEC67, 0xF893A2EEFB32555E, 0x71C18690EE42C90B]

# The error classes of a failed call, from a fixed message table; messages are never stored (section 3.1).
ERROR_CLASSES = [
    ("notimpl", ("not available", "not implemented")),
    ("solver", ("iterations", "no solution", "solver failed")),
    ("domain", ("out of range", "must be", "outside")),
]

# The `facts` sets (section 3.5): one named oracle fact per row. `call` is written with `;` because cells hold no
# commas. M1.13 adds the `register` set.
FACTS = {
    "smoke": [
        ("r134a_h_t300_q1", "PropsSI", ("H", "T", 300, "Q", 1, "R134a")),
        ("water_tcrit", "Props1SI", ("Water", "Tcrit")),
        ("water_t_reducing", "Props1SI", ("Water", "T_reducing")),
    ],
}


def fail(message):
    print(f"gen.py: {message}", file=sys.stderr)
    sys.exit(2)


def read_lock(path):
    """`key value` lines (section 3.2); `#` lines are comments."""
    lock = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        key, _, value = line.partition(" ")
        if key in lock:
            fail(f"{path}: {key} appears twice")
        lock[key] = value.strip()
    return lock


def assert_environment():
    """Assertion 1, before `import CoolProp`: no load-time switch can be set, and the locale is C."""
    switches = sorted(k for k in os.environ if k.startswith(SCRUBBED_PREFIXES))
    if switches:
        fail(f"assertion 1: these variables must not be set: {', '.join(switches)}")
    if os.environ.get("LC_ALL") != "C":
        fail("assertion 1: LC_ALL must be C")


def import_coolprop(lock):
    """Assertion 2: the wheel is the pinned one."""
    import CoolProp
    import CoolProp.CoolProp as CP

    if CoolProp.__version__ != lock["version"]:
        fail(f"assertion 2: CoolProp {CoolProp.__version__}, the lock pins {lock['version']}")
    if CoolProp.__gitrevision__ != lock["git"]:
        fail(f"assertion 2: git {CoolProp.__gitrevision__}, the lock pins {lock['git']}")
    so = Path(CoolProp.__file__).parent / lock["so_name"]
    if not so.is_file() or hashlib.sha256(so.read_bytes()).hexdigest() != lock["so_sha256"]:
        fail(f"assertion 2: {so} is missing or its sha256 differs from the lock")
    if len(CP.FluidsList()) != FLUID_COUNT:
        fail(f"assertion 2: {len(CP.FluidsList())} fluids, not {FLUID_COUNT}")
    return CP


def set_config(CP, lock):
    """Assertion 3: every config key set explicitly to the lock's value, then read back equal."""
    wanted = json.loads(lock["config_json"])
    current = json.loads(CP.get_config_as_json_string())
    if set(wanted) != set(current):
        fail(f"assertion 3: the lock's config_json must name exactly the wheel's {len(current)} keys")
    setters = [(bool, CP.set_config_bool), (int, CP.set_config_int), (float, CP.set_config_double),
               (str, CP.set_config_string)]
    for name, value in sorted(wanted.items()):
        setter = next(set_ for type_, set_ in setters if isinstance(value, type_))
        try:
            setter(getattr(CP.configuration_keys, name), value)
        except (TypeError, ValueError) as exception:
            fail(f"assertion 3: cannot set {name} to {value!r}: {exception}")
    if json.loads(CP.get_config_as_json_string()) != wanted:
        fail("assertion 3: the config read back differs from the lock")
    return canonical_json(wanted)


def fluid_files(lock):
    """Fluid name -> its file in reference/CoolProp/dev/fluids (names come from INFO.NAME, not file names), after
    checking the directory against the lock's fluids_sha256 (the sorted "file sha256" lines; part of assertion 4)."""
    paths = sorted(FLUIDS_DIR.glob("*.json"))
    listing = "".join(sorted(f"{p.name} {hashlib.sha256(p.read_bytes()).hexdigest()}\n" for p in paths))
    if hashlib.sha256(listing.encode("utf-8")).hexdigest() != lock["fluids_sha256"]:
        fail(f"assertion 4: {FLUIDS_DIR} differs from the lock's fluids_sha256 (scripts/fetch-coolprop.sh)")
    return {json.loads(p.read_text(encoding="utf-8"))["INFO"]["NAME"]: p for p in paths}


def assert_fluid(CP, files, fluid):
    """Assertion 4: the oracle's data is the repo data; returns the file's sha256 for the header."""
    path = files.get(fluid)
    if path is None:
        fail(f"assertion 4: no file for {fluid} in {FLUIDS_DIR}")
    if json.loads(CP.get_fluid_param_string(fluid, "JSON"))[0] != json.loads(path.read_text(encoding="utf-8")):
        fail(f"assertion 4: CoolProp's JSON for {fluid} differs from {path.name}")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def splitmix64(seed):
    mask = (1 << 64) - 1
    state = seed
    while True:
        state = (state + 0x9E3779B97F4A7C15) & mask
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & mask
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & mask
        yield z ^ (z >> 31)


def assert_golden():
    """Assertion 5: the sampler is the one phasekit_verify::sample implements."""
    rng = splitmix64(1)
    if [next(rng) for _ in GOLDEN] != GOLDEN:
        fail("assertion 5: SplitMix64 seed 1 does not give the golden vector")


def canonical_json(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def error_class(exception):
    message = str(exception).lower()
    for name, needles in ERROR_CLASSES:
        if any(needle in message for needle in needles):
            return name
    return "other"


def call(CP, fn, args):
    """One oracle call -> (value, status). `PropsSI` raises on failure, but `Props1SI` returns inf and leaves the
    message in the process-wide errstring (CoolProp's C convention; reading errstring clears it): both are checked."""
    CP.get_global_param_string("errstring")
    try:
        value = float(getattr(CP, fn)(*args))
    except Exception as exception:  # every oracle failure becomes a status, never a crash
        return math.nan, f"err:{error_class(exception)}"
    message = CP.get_global_param_string("errstring")
    if message:
        return math.nan, f"err:{error_class(message)}"
    return value, "ok"


def cell(value):
    """Python repr, the shortest round trip; NaN and the infinities as nan, inf, -inf."""
    if math.isnan(value):
        return "nan"
    return repr(float(value))


def fnv1a64(values):
    """FNV-1a 64 over the little-endian bits of every float cell, NaN as 0x7ff8000000000000 (section 3.3)."""
    h = 0xCBF29CE484222325
    for value in values:
        data = struct.pack("<Q", 0x7FF8000000000000) if math.isnan(value) else struct.pack("<d", value)
        for byte in data:
            h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def header(kind, lock, config, fluids, columns, tol, floats):
    generator = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    libc = "-".join(part for part in platform.libc_ver() if part) or "unknown"
    lines = [
        f"fixture: {kind}/v1",
        f"oracle: CoolProp {lock['version']} git={lock['git']} so_sha256={lock['so_sha256']}",
        f"generator: gen.py sha256={generator} python={platform.python_version()} libc={libc}",
        f"config: {config}",
        "env: scrubbed COOLPROP_* PXFLASH_*; LC_ALL=C",
        "fluid: " + " ".join(f"{name} json_sha256={sha}" for name, sha in fluids),
        "source: coolprop",
        "columns: " + ",".join(columns),
        "tol: " + ",".join(tol),
        f"bits: fnv1a64={fnv1a64(floats)}",
    ]
    return "".join(f"# {line}\n" for line in lines)


def facts(CP, lock, config, files, args):
    rows = FACTS.get(args.set)
    if rows is None:
        fail(f"--set must be one of {', '.join(sorted(FACTS))} for --kind facts")
    fluids = sorted({inputs[-1] if fn == "PropsSI" else inputs[0] for _, fn, inputs in rows})
    shas = [(fluid, assert_fluid(CP, files, fluid)) for fluid in fluids]
    lines, floats = [], []
    for name, fn, inputs in rows:
        value, status = call(CP, fn, inputs)
        floats.append(value)
        lines.append(f"{name},{fn}({';'.join(str(a) for a in inputs)}),{status},{cell(value)}\n")
    columns, tol = ["name", "call", "status", "value"], ["label", "label", "label", "exact"]
    text = header("facts", lock, config, shas, columns, tol, floats)
    return {f"facts/{args.set}.csv": text + "".join(lines)}


GENERATORS = {"facts": facts}


def write(out, files):
    """Each file to a temp name in its directory, then renamed."""
    for rel, text in sorted(files.items()):
        path = Path(out) / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        os.replace(tmp, path)


def check(out, files):
    """--check: compare with what DIR holds, byte for byte."""
    differ = [rel for rel, text in sorted(files.items()) if not (Path(out) / rel).is_file()
              or (Path(out) / rel).read_bytes() != text.encode("utf-8")]
    for rel in differ:
        print(f"gen.py: {Path(out) / rel} differs from a fresh generation", file=sys.stderr)
    return 1 if differ else 0


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lock", required=True)
    parser.add_argument("--kind", required=True, choices=KINDS)
    parser.add_argument("--set", default="smoke")
    parser.add_argument("--tier", choices=["core", "all", "full"], default="core")
    parser.add_argument("--fluids")
    parser.add_argument("--rows", type=int)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--jobs", type=int, default=0)
    parser.add_argument("--out", required=True)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--write-manifest", action="store_true")
    return parser.parse_args(argv)


def main(argv):
    args = parse_args(argv)
    assert_environment()
    lock = read_lock(args.lock)
    if args.write_manifest:
        fail("--write-manifest lands at PLAN.md M1.4")
    generate = GENERATORS.get(args.kind)
    if generate is None:
        fail(f"--kind {args.kind} is added by the PLAN.md step that first uses it")
    if Path(args.out).resolve().is_relative_to(ROOT / "reference"):
        fail("nothing is written under reference/")
    CP = import_coolprop(lock)
    config = set_config(CP, lock)
    assert_golden()
    files = generate(CP, lock, config, fluid_files(lock), args)
    if args.check:
        return check(args.out, files)
    write(args.out, files)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
