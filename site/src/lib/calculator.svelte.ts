// The calculator's view (PLAN.md M21.1, M21.7): a fluid, two inputs and the units, kept in SI, and the state the engine
// computes from them. The component only binds to it; tests/unit/calculator.svelte.test.ts drives it, wiring included.
// The view lives in the URL (`fluid=Water&a=P:101325&b=Q:0&units=engineering`, values in SI).
import { untrack } from "svelte";
import type { Engine, Refusal, StateValue, TimeValue } from "../engine/protocol";
import { formatNumber } from "./format";
import type { Preset } from "./fluids";
import { INPUTS, OUTPUTS, output } from "./outputs";
import { refusalOf, say } from "./refusal";
import { displayUnit, fromSI, toSI, type System, type Unit } from "./units";

/** Every output key the calculator asks for. */
export const KEYS: readonly string[] = OUTPUTS.map((o) => o.key);

const DEFAULT = { fluid: "Water", a: ["P", 101325], b: ["Q", 0] } as const;

type Which = "a" | "b";

/** An input from the URL, `key:value` in SI, or the fallback for anything else. */
function readInput(raw: string | null, fallback: readonly [string, number]): [string, number] {
  const [key, text] = String(raw).split(":") as [string, string?];
  return INPUTS.includes(key) && text !== undefined && !invalid(text)
    ? [key, Number(text)]
    : [fallback[0], fallback[1]];
}

/** The unit of a key no output names: none. */
const NO_UNIT: Unit = { label: "", scale: 1, offset: 0 };

/** Whether an input field's text is not a finite number. */
export function invalid(text: string): boolean {
  return text.trim() === "" || !Number.isFinite(Number(text));
}

export class Calculator {
  // The view, in SI. The fields' text is kept apart, so typing is never reformatted under the cursor.
  fluid = $state<string>(DEFAULT.fluid);
  aKey = $state<string>(DEFAULT.a[0]);
  aValue = $state<number>(DEFAULT.a[1]);
  bKey = $state<string>(DEFAULT.b[0]);
  bValue = $state<number>(DEFAULT.b[1]);
  system = $state<System>("si");
  aText = $state("");
  bText = $state("");

  // The state: the latest request's answer, or its refusal, which the package explains in the shown units.
  result = $state<StateValue | null>(null);
  refusal = $state<Refusal | null>(null);
  busy = $state(true);
  timing = $state<TimeValue | null>(null);

  /** The answered outputs, and the package's reason for each refused one, short and in the shown units. */
  readonly values = $derived.by(() => {
    const map = new Map<string, number>();
    const errors = new Map<string, string>();
    for (const [i, key] of KEYS.entries()) {
      const value = this.result?.values[i];
      const refused = this.result?.errors[i];
      if (typeof value === "number" && !Number.isNaN(value)) map.set(key, value);
      else if (this.result) errors.set(key, refused ? say(refused, this.system, "short") : "not defined at this state");
    }
    return { map, errors };
  });

  /** The refusal of the state in words, in the shown units (PLAN.md M28.3); "" while there is none. */
  readonly refusalText = $derived(this.refusal ? say(this.refusal, this.system, "long") : "");

  readonly #engine: Engine;
  readonly #timingDelayMs: number;
  #sequence = 0;

  /** `timingDelayMs`: how long a state must stay settled before the engine times it. */
  constructor(engine: Engine, timingDelayMs = 300) {
    this.#engine = engine;
    this.#timingDelayMs = timingDelayMs;
  }

  /** The view a URL's params name; anything malformed falls back to the default. */
  load(params: URLSearchParams): void {
    this.fluid = params.get("fluid") ?? DEFAULT.fluid;
    [this.aKey, this.aValue] = readInput(params.get("a"), DEFAULT.a);
    [this.bKey, this.bValue] = readInput(params.get("b"), DEFAULT.b);
    this.system = params.get("units") === "engineering" ? "engineering" : "si";
    this.#showInputs();
  }

  /** The view as URL params; the units only when they are not SI. */
  params(): URLSearchParams {
    const params = new URLSearchParams({
      fluid: this.fluid,
      a: `${this.aKey}:${this.aValue}`,
      b: `${this.bKey}:${this.bValue}`,
    });
    if (this.system === "engineering") params.set("units", this.system);
    return params;
  }

  /** The unit a key shows in. */
  unitOf(key: string): Unit {
    const o = output(key);
    return o ? displayUnit(o.kind, this.system) : NO_UNIT;
  }

  /** An input field's new text: the value changes only when the text is a finite number, in the shown unit. */
  typed(which: Which, text: string): void {
    this[`${which}Text`] = text;
    if (invalid(text)) return;
    this[`${which}Value`] = toSI(Number(text), this.unitOf(this[`${which}Key`]));
  }

  /** A new variable for an input keeps the state: its value is the current state's, where the package gave one. */
  rekey(which: Which, key: string): void {
    this[`${which}Value`] = this.values.map.get(key) ?? this[`${which}Value`];
    this[`${which}Key`] = key;
    this.#showInputs();
  }

  setSystem(system: System): void {
    this.system = system;
    this.#showInputs();
  }

  /** Shows the state of `fluid` at the same inputs: "did you mean" (PLAN.md M29.7). */
  pick(fluid: string): void {
    this.fluid = fluid;
  }

  apply(preset: Preset): void {
    this.fluid = preset.fluid;
    [this.aKey, this.aValue] = preset.a;
    [this.bKey, this.bValue] = preset.b;
    this.#showInputs();
  }

  /** The wiring: the route loads the view, the view writes the URL, the inputs compute the state, and a settled state
   *  is timed. Returns the function that disconnects it. */
  connect(route: { readonly params: URLSearchParams }, write: (params: URLSearchParams) => void): () => void {
    return $effect.root(() => {
      // The route is this effect's only dependency: what it sets is untracked, or every edit would re-run it and
      // restore the URL's old view.
      $effect(() => {
        const params = route.params;
        untrack(() => this.load(params));
      });
      $effect(() => write(this.params()));
      $effect(() => this.#compute());
      $effect(() => this.#scheduleTiming());
    });
  }

  #shown(key: string, value: number): string {
    const x = fromSI(value, this.unitOf(key));
    return Number.isFinite(x) ? String(Number(x.toPrecision(12))) : "";
  }

  #showInputs(): void {
    this.aText = this.#shown(this.aKey, this.aValue);
    this.bText = this.#shown(this.bKey, this.bValue);
  }

  #request() {
    return {
      fluid: this.fluid,
      name1: this.aKey,
      value1: this.aValue,
      name2: this.bKey,
      value2: this.bValue,
      outputs: [...KEYS],
    };
  }

  // The state, from the engine; a late answer to an older request is dropped.
  #compute(): void {
    const request = this.#request();
    // Stryker disable next-line UpdateOperator: any counter that gives each request its own number works; down as up.
    const mine = ++this.#sequence;
    untrack(() => (this.busy = true));
    this.#engine.call({ op: "state", ...request }).then(
      (value) => {
        if (mine === this.#sequence) [this.result, this.refusal, this.busy] = [value, null, false];
      },
      (e: unknown) => {
        if (mine === this.#sequence) [this.result, this.refusal, this.busy] = [null, refusalOf(e), false];
      },
    );
  }

  // Once a state has settled, the engine times it. A result is the latest request's, so the inputs read are its own.
  #scheduleTiming(): (() => void) | undefined {
    if (!this.result) {
      this.timing = null;
      return undefined;
    }
    const request = untrack(() => this.#request());
    const mine = this.#sequence;
    const timer = setTimeout(() => {
      this.#engine.call({ op: "time", ...request }).then(
        (value) => {
          if (mine === this.#sequence) this.timing = value;
        },
        () => {},
      );
    }, this.#timingDelayMs);
    return () => clearTimeout(timer);
  }
}
