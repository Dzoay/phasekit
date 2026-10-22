# Error message wording (2026-10-22)

Asked while M28.1 was built and unpushed: every message had been shown in both styles (user decision EM2), and four
points of its wording were open. The long forms named properties and strategies by their Rust variants
(`Cpmass is undefined for a two-phase state`, `DensityNewton did not converge in 50 iterations`, `no model for
Viscosity`); an `InvalidInput` that breaks none of the named rules said "out of range"; a structural argument's short
form was its long one (`invalid number of mole fractions: 3`); numbers from 1e-4 to below 1e6 were written as decimals
(`100000 Pa`). The user asked which form serves an agent using the library best. The answer given: an agent does best
on the structured error (the Rust `Error`, the C ABI's `Status`, the browser package's `Refusal` handle), and among the
strings on the short form, whose keys are the API's own.

| # | Question | Decision | Notes |
|---|---|---|---|
| EM7 | How does a message name a property, a constant or a strategy? | **The long form in words with the key beside it, `the isobaric heat capacity (Cpmass)`; the short form by the key alone, `Cpmass: undefined, two-phase`** | "I think both is a good idea (long for humans, short for agents?)", then "Words + key". The key carries the basis, so a molar and a mass property share their words. A partial derivative is its notation, after "the derivative" in the long form. An unsupported pair's long form is unchanged (MX4). *Rejected:* the keys in both forms; words alone in the long form. |
| EM8 | Does an `InvalidInput` that breaks no named rule say "out of range"? | **Yes** | Core names a rule only when the value breaks it; it does not know the bound otherwise. *Rejected:* "is not accepted by this model". |
| EM9 | May a structural argument's short form equal its long one? | **Yes** | `invalid number of mole fractions: 3`, `invalid plane`: already short, with no relation to state. |
| EM10 | Which numbers are written as decimals? | **From 1e-4 to below 1e6, as built; others as `2e9`** | `100000 Pa` and `101325 Pa` read as engineers type them. *Rejected:* decimals only below 1e5. |
