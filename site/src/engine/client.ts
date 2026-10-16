// The page's side of the engine: one worker, started on first use, and a promise per request.
import type { Body, Engine, Response, Values } from "./protocol";

/** A refusal: the package's message, or the worker's if it could not load. */
export class EngineError extends Error {}

class WorkerEngine implements Engine {
  #worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  #next = 1;
  #pending = new Map<number, (response: Response) => void>();

  constructor() {
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

let engine: WorkerEngine | undefined;

export function getEngine(): Engine {
  engine ??= new WorkerEngine();
  return engine;
}
