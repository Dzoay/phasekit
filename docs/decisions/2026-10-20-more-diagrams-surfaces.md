# More diagrams: the surfaces (2026-10-20)

Asked when M26 was planned (user decision DG6, PR #272), with three questions left open in PLAN.md §6: how the site
draws the 3-D surface, either with a projection it computes and draws itself, adding no dependency, or with a WebGL
library such as three.js, which DEPENDENCIES.md's tiers would have to admit; whether Gibbs's u–s–v surface joins the
p–v–T one; and the order of M26's three sets. The proposal was the site's own projection, p–v–T alone, and the
surfaces first.

| # | Question | Decision | Notes |
|---|---|---|---|
| DG7 | How does the site draw the 3-D surfaces? | **With three.js, WebGL** | "lets admit three.js for WebGL rendering": three.js joins DEPENDENCIES.md's tier T5, bundled into the page, its version pinned at M26.2. It is MIT-licensed, which `deny.toml` allows. *Rejected:* a projection the site draws itself. |
| DG8 | Does Gibbs's u–s–v surface join the p–v–T one? | **Yes, as an option beside p–v–T** | "Add Gibbs u-s-v surface, nice to have options". The page offers both. |
| DG9 | The order of M26's three sets | **The surfaces first, as the plan has it** | "Surfaces first as the plan says": the surfaces (M26.1, M26.2), then the curves and the property maps (M26.3, M26.4), then the ternaries (M26.5). |
