// The sweeps page's view (PLAN.md M21.3): a fluid, a mode, the output plotted and the lines' held values, in the URL
// (`fluid=n-Propane&mode=isobars&y=Cpmass&at=2100000,4700000&units=engineering`, the held values in SI); the sweep,
// sampled once per fluid, mode, output and values. Without `at` the lines are the defaults about the critical point.
// tests/unit/sweeps-view.svelte.test.ts drives it.
import { untrack } from "svelte";
import type { Engine } from "../engine/protocol";
import { output, type Output } from "../lib/outputs";
import { displayUnit, fromSI, toSI, type System, type Unit } from "../lib/units";
import type { Batch } from "../sample/lines";
import { LIMIT_KEYS, limits } from "../sample/ranges";
import { LATENT, MAX_LINES, MODES, defaultValues, sampleSweep, type Sweep, type SweepMode } from "../sample/sweeps";

const DEFAULT = { fluid: "n-Propane", mode: "isobars", y: "Cpmass" } as const;

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** The unit an output shows in: the latent heat's is an enthalpy's. */
export function unitOfOutput(key: string, system: System): Unit {
  return displayUnit((output(key === LATENT ? "Hmass" : key) as Output).kind, system);
}

/** The unit a mode's held values show in: an isobar's p, an isotherm's T. */
export function heldUnit(mode: SweepMode, system: System): Unit {
  return unitOfOutput(mode === "isotherms" ? "T" : "P", system);
}

/** Held values from the URL: up to MAX_LINES finite, positive numbers; null where there are none. */
function parseAt(raw: string | null): number[] | null {
  const values = String(raw)
    .split(",")
    .map(Number)
    .filter((v) => Number.isFinite(v) && v > 0)
    .slice(0, MAX_LINES);
  return values.length > 0 ? values : null;
}

export class SweepView {
  fluid = $state<string>(DEFAULT.fluid);
  mode = $state<SweepMode>(DEFAULT.mode);
  /** The output plotted: an output key, or the latent heat along saturation. */
  y = $state<string>(DEFAULT.y);
  system = $state<System>("si");
  /** The lines' held values in SI, or null for the defaults. */
  at = $state.raw<number[] | null>(null);

  data = $state.raw<Sweep | null>(null);
  refusal = $state<string | null>(null);
  busy = $state(true);

  readonly #engine: Engine;
  readonly #cache = new Map<string, Sweep>();
  #sequence = 0;

  constructor(engine: Engine) {
    this.#engine = engine;
  }

  /** The view a URL's params name; anything malformed falls back to n-Propane's c_p along isobars, in SI. */
  load(params: URLSearchParams): void {
    this.fluid = params.get("fluid") ?? DEFAULT.fluid;
    this.mode = MODES.find((m) => m === params.get("mode")) ?? DEFAULT.mode;
    const y = String(params.get("y"));
    this.y = output(y) || (y === LATENT && this.mode === "saturation") ? y : DEFAULT.y;
    this.system = params.get("units") === "engineering" ? "engineering" : "si";
    this.at = this.mode === "saturation" ? null : parseAt(params.get("at"));
  }

  params(): URLSearchParams {
    const params = new URLSearchParams({ fluid: this.fluid, mode: this.mode, y: this.y });
    if (this.system === "engineering") params.set("units", this.system);
    if (this.at) params.set("at", this.at.join(","));
    return params;
  }

  /** A new mode starts from its default lines; only saturation has the latent heat. */
  setMode(mode: SweepMode): void {
    this.mode = mode;
    this.at = null;
    if (mode !== "saturation" && this.y === LATENT) this.y = DEFAULT.y;
  }

  setSystem(system: System): void {
    this.system = system;
  }

  /** The held values as typed, in the shown units, separated by commas or spaces; nothing typed means the defaults. */
  setValues(text: string): void {
    const unit = heldUnit(this.mode, this.system);
    // Stryker disable next-line Regex: an empty piece between separators reads as 0, which parseAt drops.
    const values = text.split(/[\s,;]+/).map((t) => toSI(Number(t), unit));
    this.at = parseAt(values.join(","));
  }

  /** The lines' held values in the shown units, as the input shows them. */
  heldText(): string {
    const unit = heldUnit(this.mode, this.system);
    const values = this.at ?? this.data?.lines.map((l) => l.value) ?? [];
    return values.map((v) => String(Number(fromSI(v, unit).toPrecision(6)))).join(", ");
  }

  /** The wiring: the route loads the view, the view writes the URL, and its settings load their sweep. */
  connect(route: { readonly params: URLSearchParams }, write: (params: URLSearchParams) => void): () => void {
    return $effect.root(() => {
      $effect(() => {
        const params = route.params;
        untrack(() => this.load(params));
      });
      $effect(() => write(this.params()));
      $effect(() => this.#sample(this.fluid, this.mode, this.y, this.at));
    });
  }

  // The sweep: from the cache, or the fluid's limits and then its sampling; a late answer is dropped.
  #sample(fluid: string, mode: SweepMode, y: string, at: number[] | null): void {
    const key = `${fluid}/${mode}/${y}/${String(at)}`;
    // Stryker disable next-line UpdateOperator: as the diagrams view's.
    const mine = ++this.#sequence;
    const cached = this.#cache.get(key);
    if (cached) {
      [this.data, this.refusal, this.busy] = [cached, null, false];
      return;
    }
    untrack(() => (this.busy = true));
    const batch: Batch = (outputs, name1, values1, name2, values2) =>
      this.#engine.call({ op: "batch", fluid, outputs, name1, values1, name2, values2 });
    this.#engine
      .call({ op: "constants", fluid, keys: [...LIMIT_KEYS] })
      .then((values) => {
        const l = limits(values);
        if (!l) throw new Error(`${fluid} names no critical or triple point to sweep about`);
        return sampleSweep(mode, y, at ?? defaultValues(mode, l), l, batch);
      })
      .then(
        (data) => {
          this.#cache.set(key, data);
          if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [data, null, false];
        },
        (e: unknown) => {
          if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [null, message(e), false];
        },
      );
  }
}
