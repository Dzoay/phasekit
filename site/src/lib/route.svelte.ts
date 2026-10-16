// The page's route, `#/<page>?<params>`. Hash routes need no server rewrites on GitHub Pages (user decision WS2), and
// the params hold the view's state, so a link reproduces the view.

export interface Route {
  page: string;
  params: URLSearchParams;
}

export function parseRoute(hash: string): Route {
  const [path = "", query = ""] = hash.replace(/^#\/?/, "").split("?", 2);
  return { page: path || "calculator", params: new URLSearchParams(query) };
}

/** The current route; it changes when the hash does (a link, the back button), not when a page writes its params. */
export const route: Route = $state(parseRoute(location.hash));

addEventListener("hashchange", () => Object.assign(route, parseRoute(location.hash)));

/** Record a page's state in the URL, with no history entry and no hashchange. */
export function writeParams(page: string, params: URLSearchParams): void {
  history.replaceState(history.state, "", `#/${page}?${params}`);
}
