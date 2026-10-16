// The current route, following the browser's hash (routes.ts parses it). Browser glue: it reads `location` and
// `history` when imported, so Playwright drives it and the mutants gate skips it (scripts/site/mutants.sh).
import { parseRoute, type Route } from "./routes";

/** The current route; it changes when the hash does (a link, the back button), not when a page writes its params. */
export const route: Route = $state(parseRoute(location.hash));

addEventListener("hashchange", () => Object.assign(route, parseRoute(location.hash)));

/** Record a page's state in the URL, with no history entry and no hashchange. */
export function writeParams(page: string, params: URLSearchParams): void {
  history.replaceState(history.state, "", `#/${page}?${params}`);
}
