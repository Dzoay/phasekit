# The property maps' bands (2026-10-22)

Asked at the start of M26.4. The plan had the library compute a property map's contour lines and the site fill the
bands between them, but contour lines are not shapes a page can fill: closing a band takes the range's edges, the gaps
and the order of the lines around them, which is geometry, and the site draws only what the library computed (user
decision WS4). The proposal was for the library to give each band as polygons too.

| # | Question | Decision | Notes |
|---|---|---|---|
| DG11 | What does the library return for a property map, so that the site can fill its bands? | **The contour lines, and each grid cell's piece of each band as a polygon** | Each polygon's corners are states the library computed: the cell's corners, which are grid states, and the crossings on its edges, which are the contour lines' vertices. The site fills the polygons and draws the lines; it computes no geometry. *Rejected:* lines only, which would leave M26.6 to come back for polygons; the grid's values for the site to colour whole cells by band, which is coarse and leaves the band to the site. |
