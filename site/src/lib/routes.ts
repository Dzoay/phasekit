// The page's routes, `#/<page>?<params>`: hash routes need no server rewrites on GitHub Pages (user decision WS2), and
// the params hold the view's state, so a link reproduces the view. route.svelte.ts follows the browser's hash with it.

export interface Route {
  page: string;
  params: URLSearchParams;
}

/** The route a location hash names; an empty one is the calculator's. */
export function parseRoute(hash: string): Route {
  const rest = hash.replace(/^#\/?/, "");
  const at = rest.indexOf("?");
  const [path, query] = at < 0 ? [rest, ""] : [rest.slice(0, at), rest.slice(at + 1)];
  return { page: path || "calculator", params: new URLSearchParams(query) };
}
