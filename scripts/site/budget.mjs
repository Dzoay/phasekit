// budget.mjs <dist> <budget-file>: the page's JavaScript and CSS, gzipped, against its budget (PLAN.md M21.1): the
// first green size + 10 %, as the wasm modules' (user decision SL8). The .wasm modules have their own budgets
// (crates/phasekit-wasm/size-budget.txt) and the font is not code, so neither counts here.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

const [dist, budgetFile] = process.argv.slice(2);
if (!dist || !budgetFile) {
  console.error("usage: budget.mjs <dist> <budget-file>");
  process.exit(2);
}

const files = readdirSync(join(dist, "assets")).filter((f) => /\.(js|css)$/.test(f));
const bytes = files.reduce((sum, f) => sum + gzipSync(readFileSync(join(dist, "assets", f)), { level: 9 }).length, 0);
const budget = Number(/^js\+css-gzip (\d+)$/m.exec(readFileSync(budgetFile, "utf8"))?.[1]);
if (!Number.isFinite(budget)) {
  console.error(`budget: no "js+css-gzip <bytes>" line in ${budgetFile}; measured ${bytes}`);
  process.exit(1);
}
console.log(`budget: ${files.length} files, ${bytes} gzipped bytes of ${budget}`);
if (bytes > budget) {
  console.error(`budget: over by ${bytes - budget} bytes; raise it only with a reason written in ${budgetFile}`);
  process.exit(1);
}
