# Paper sources (2026-10-05)

Asked during M1.10: the green-OA copies of Lemmon et al. 2015 and Thol and Lemmon 2016 are NIH author manuscripts that
PMC serves to scripts only as JATS XML, under the statement "This file is available for text mining. It may also be
used consistent with the principles of fair use under the copyright law." (PMC's metadata code `TDM`; not an open
licence, copyright stays with the publishers). Can they be used here?

| # | Question | Decision | Notes |
|---|---|---|---|
| PS1 | PMC author manuscripts as the source of transcribed check values | **Yes, as is** | Fetched anonymously from the PMC Cloud Service (`pmc-oa-opendata`), one of the routes PMC allows for automated retrieval; kept in the gitignored `reference/papers/`, never committed. Only the table's numbers and the citation are committed, on the basis of VERIFICATION.md §3.7 (numerical facts; user decision 3b), not on the text-mining licence. The citation gives `manuscript=` and `xml_sha256=` (VERIFICATION.md §4.2). Publisher PDFs the user supplies remain an alternative. |
