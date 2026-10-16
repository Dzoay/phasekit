// The viewer's theme choice: kept in storage when the browser allows it, and never needed for the page to work.
import { describe, expect, test } from "vitest";
import {
  ACCENTS,
  DEFAULT_THEME,
  MODES,
  STORAGE_KEY,
  applyTheme,
  loadTheme,
  parseTheme,
  saveTheme,
  type Theme,
} from "../../src/lib/theme";

function memory(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (i) => [...items.keys()][i] ?? null,
    removeItem: (key) => void items.delete(key),
    setItem: (key, value) => void items.set(key, value),
  };
}

const blocked = (): Storage => {
  throw new DOMException("The operation is insecure.", "SecurityError");
};

describe("theme", () => {
  test("parses what it saves and nothing else", () => {
    for (const mode of MODES) {
      for (const accent of ACCENTS) {
        const theme: Theme = { mode, accent };
        expect(parseTheme(JSON.stringify(theme))).toEqual(theme);
      }
    }
    for (const raw of [null, "", "{", "[]", '{"mode":"sepia","accent":"teal"}', '{"mode":"dark","accent":"plaid"}']) {
      expect(parseTheme(raw), String(raw)).toEqual(DEFAULT_THEME);
    }
  });

  test("round-trips through storage", () => {
    const storage = memory();
    const theme: Theme = { mode: "dark", accent: "amber" };
    expect(saveTheme(theme, () => storage)).toBe(true);
    expect(loadTheme(() => storage)).toEqual(theme);
  });

  test("works without storage", () => {
    expect(loadTheme(blocked)).toEqual(DEFAULT_THEME);
    expect(saveTheme({ mode: "light", accent: "rose" }, blocked)).toBe(false);
  });
});

describe("applying a theme", () => {
  test("follows the system, in teal, until the viewer chooses", () => {
    expect(DEFAULT_THEME).toEqual({ mode: "system", accent: "teal" });
  });

  test("sets <html>'s data-theme and data-accent, which app.css reads", () => {
    const root = { dataset: {} as DOMStringMap } as HTMLElement;
    applyTheme({ mode: "dark", accent: "rose" }, root);
    expect({ ...root.dataset }).toEqual({ theme: "dark", accent: "rose" });
  });

  test("keeps the choice under the key index.html reads", () => {
    const storage = memory();
    saveTheme({ mode: "light", accent: "indigo" }, () => storage);
    expect(STORAGE_KEY).toBe("phasekit.theme");
    expect(storage.getItem("phasekit.theme")).toBe('{"mode":"light","accent":"indigo"}');
  });
});
