// The page's side of the engine: a worker, started on first use, and a promise per request. The page has one worker on
// the build its browser runs best; the benchmark one more for each build (PLAN.md M21.3).
import type { Body, Build, Engine, Response, Values } from "./protocol";
import { simdSupported } from "./simd";

/** A refusal: the package's message, or the worker's if it could not load. */
export class EngineError extends Error {}

class WorkerEngine implements Engine {
  #worker: Worker;
  #next = 1;
  #pending = new Map<number, (response: Response) => void>();

  /** `build`, or "" for the best this browser runs. */
  constructor(build: Build | "") {
    this.#worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module", name: build });
    this.#worker.onmessage = (event: MessageEvent<Response>) => {
      const done = this.#pending.get(event.data.id);
      this.#pending.delete(event.data.id);
      done?.(event.data);
    };
    this.#worker.onerror = (event) => {
      for (const [id, done] of this.#pending)
        done({ id, ok: false, op: "info", error: event.message || "the engine stopped" });
      this.#pending.clear();
    };
  }

  call<B extends Body>(body: B): Promise<Values[B["op"]]> {
    const id = this.#next++;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, (r) => (r.ok ? resolve(r.value as Values[B["op"]]) : reject(new EngineError(r.error))));
      this.#worker.postMessage({ ...body, id });
    });
  }
}

const engines = new Map<Build | "", WorkerEngine>();

/** The engine on `build`, or on the best build this browser runs; one worker for each, started on first use. */
export function getEngine(build: Build | "" = ""): Engine {
  let engine = engines.get(build);
  if (!engine) {
    engine = new WorkerEngine(build);
    engines.set(build, engine);
  }
  return engine;
}

/** The builds this browser runs: the baseline everywhere, simd128 where it validates WebAssembly SIMD. */
export function builds(): Build[] {
  return simdSupported() ? ["baseline", "simd128"] : ["baseline"];
}
