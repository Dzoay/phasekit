// The colour tokens of src/app.css as the contrast test reads them. Each token is written once, as
// `--name: light-dark(#rrggbb, #rrggbb);`: the neutral ones in the block whose selector is `:root`, an accent's in a
// block whose selector names `[data-accent="…"]`. A theme is a scheme (light or dark) with one accent.

export interface Theme {
  name: string;
  tokens: Map<string, string>;
}

const BLOCK = /([^{}]+)\{([^}]*)\}/g;
const ACCENT = /\[data-accent="([a-z]+)"\]/;
const TOKEN = /(--[a-z0-9-]+):\s*light-dark\(\s*(#[0-9a-f]{6})\s*,\s*(#[0-9a-f]{6})\s*\)/g;

function tokens(body: string, scheme: 0 | 1): Map<string, string> {
  const out = new Map<string, string>();
  for (const m of body.matchAll(TOKEN)) {
    const [, name, light, dark] = m;
    if (name && light && dark) out.set(name, scheme === 0 ? light : dark);
  }
  return out;
}

/** Every scheme (light, dark) with every accent: the neutral tokens, then the accent's. */
export function themes(css: string): Theme[] {
  const blocks = [...css.matchAll(BLOCK)].map((m) => [(m[1] ?? "").trim(), m[2] ?? ""] as const);
  const neutral = blocks.filter(([selector]) => selector === ":root").map(([, body]) => body);
  const accents = blocks.flatMap(([selector, body]) => {
    const accent = ACCENT.exec(selector)?.[1];
    return accent ? [[accent, body] as const] : [];
  });
  const out: Theme[] = [];
  for (const scheme of [0, 1] as const) {
    const root = new Map(neutral.flatMap((body) => [...tokens(body, scheme)]));
    for (const [accent, body] of accents) {
      out.push({
        name: `${scheme === 0 ? "light" : "dark"}, ${accent}`,
        tokens: new Map([...root, ...tokens(body, scheme)]),
      });
    }
  }
  return out;
}
