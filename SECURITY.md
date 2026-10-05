# Security policy

## Supported versions

phasekit has not been released yet. From v0.1 on, security fixes go into the latest release only: the latest `0.y`
before 1.0, then the latest minor version of the current major version.

## Reporting a vulnerability

Please report privately, through GitHub's private vulnerability reporting: on this repository open
**Security → Report a vulnerability** (or go to
[github.com/Dzoay/phasekit/security/advisories/new](https://github.com/Dzoay/phasekit/security/advisories/new)).
Do not open a public issue.

Please include the affected version or commit, the interface (Rust API, C ABI, C compatibility shim, WebAssembly),
steps or a minimal input that reproduces it, and the impact you expect.

This is a volunteer project with a single maintainer. Reports are handled on a best-effort basis; the fix and the
advisory are published together.

## Scope

In scope:

- memory-safety bugs, in particular in `phasekit-capi`, the only crate allowed to use `unsafe`;
- panics or unwinding that cross the C ABI;
- crashes, hangs or unbounded memory use caused by crafted input, including runtime-loaded data packs and the
  `PropsSI`-style string grammar.

Out of scope:

- numerical inaccuracy: open a normal issue (**Bug report** or **Literature disagrees**);
- vulnerabilities in CoolProp itself: report them to the [CoolProp project](https://github.com/CoolProp/CoolProp);
- weaknesses of a WebAssembly runtime or browser sandbox.
