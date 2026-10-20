# Diagram geometry (2026-10-20)

Asked when M21 could not close without M19: M21.6, its last step, draws the diagrams from M19's geometry. M19 had a
roadmap row and no steps, its crate "named at its plan" (user decision WS4). The site samples its own geometry in
TypeScript through the browser package's batch: domes and isolines (`site/src/sample/lines.ts`, M21.2), a mixture's
envelope (`envelope.ts`, M21.5) and the phase regions (`regions.ts`, M21.9), beside the sweeps (`sweeps.ts`, M21.3),
which M21.6's text, written before M21.3 and M21.9, did not name. A mixture's envelope is traced by `phasekit-mix`
(M13.12), and `gates deps` holds a crate on core's public API to `phasekit-core` alone.

| # | Question | Decision | Notes |
|---|---|---|---|
| DG1 | The crate's name | **`phasekit-diagram`** | It computes geometry and draws nothing (WS4), so not `-plot`. *Rejected:* `phasekit-plot`, `phasekit-geometry`. |
| DG2 | Which of the site's samplers does M19 take over? | **The domes, isolines and envelopes, and the phase regions; the sweeps stay in the site** | The regions are geometry, edged by the library's lines, with a bisection on the cold edge (M21.9), so the page should not compute them. A sweep is one batch call a line, with nothing to split or refine, so it stays in the site, moved out of `site/src/sample/` when M21.6 deletes it. *Rejected:* the regions left in the site. |
| DG3 | How does a mixture's envelope reach the crate? | **As data: the crate depends on `phasekit-core` alone, and its caller passes the traced envelope** | The browser package, which depends on both, fills it from `phasekit-mix`'s `Envelope`, as WS4's "on core's public API with no new dependency" asks. The crate's tests may build traces through `phasekit-mix` as a dev-dependency. *Rejected:* a dependency on `phasekit-mix`, behind a feature or not. |
| DG4 | How do M19's steps land? | **Four PRs: M19.1 and M19.2 together, M19.3 and M19.4 together, then M19.5, then M19.6; a commit and a `Plan-Step:` footer each** | As LM7 bundled M25.1-M25.3, to save CI runs. The user agreed to the pairs as proposed; the regions, added by DG2, and the browser package's step, across crates, take a PR each *(decision)*. |
