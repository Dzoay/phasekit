// The page's side of the engine: a worker, started on first use, and a promise per request. The page has one worker on
// the build its browser runs best; the benchmark one more for each build (PLAN.md M21.3). A request that names a fluid
// first adds the packs that fluid needs (PLAN.md M21.4): each pack goes to every worker, and a worker that starts later
// is given every pack added so far before any request.
import { FluidData } from "../lib/fluiddata.svelte";
import type { PackIndex } from "../lib/packs";
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

const workers = new Map<Build | "", WorkerEngine>();
/** Every pack added, in order. */
const packs: Uint8Array[] = [];

/** The worker on `build`, started on first use with every pack added so far. A worker answers in the order it is asked,
 *  so those packs are in its registry before any later request. */
function worker(build: Build | ""): WorkerEngine {
  let w = workers.get(build);
  if (!w) {
    w = new WorkerEngine(build);
    for (const bytes of packs) w.call({ op: "pack", bytes }).catch(() => {});
    workers.set(build, w);
  }
  return w;
}

/** Fetches a file beside the page, refusing what the server did not serve. */
async function fetched(path: string): Promise<globalThis.Response> {
  const response = await fetch(path);
  if (!response.ok) throw new Error(`${path}: ${response.status} ${response.statusText}`);
  return response;
}

/** The page's fluids: packs/index.json and the packs beside it, added to every worker. */
export const fluidData = new FluidData(
  {
    index: async () => (await (await fetched("packs/index.json")).json()) as PackIndex,
    pack: async (file) => new Uint8Array(await (await fetched(`packs/${file}`)).arrayBuffer()),
  },
  {
    add(bytes) {
      const main = worker("");
      packs.push(bytes);
      for (const w of workers.values()) if (w !== main) w.call({ op: "pack", bytes }).catch(() => {});
      return main.call({ op: "pack", bytes });
    },
    contents: () => worker("").call({ op: "registry" }),
    components: (name) => worker("").call({ op: "components", name }),
    mixtures: () => worker("").call({ op: "mixtures" }),
  },
);

/** An engine that adds the packs a request's fluid needs before the request, and then asks the registry what it has
 *  decoded. */
class FluidEngine implements Engine {
  readonly #worker: WorkerEngine;

  constructor(w: WorkerEngine) {
    this.#worker = w;
  }

  async call<B extends Body>(body: B): Promise<Values[B["op"]]> {
    if (!("fluid" in body)) return this.#worker.call(body);
    await fluidData.ensure(body.fluid);
    try {
      return await this.#worker.call(body);
    } finally {
      fluidData.touch();
    }
  }
}

const engines = new Map<Build | "", FluidEngine>();

/** The engine on `build`, or on the best build this browser runs; one worker for each, started on first use. */
export function getEngine(build: Build | "" = ""): Engine {
  let engine = engines.get(build);
  if (!engine) {
    engine = new FluidEngine(worker(build));
    engines.set(build, engine);
  }
  return engine;
}

/** The builds this browser runs: the baseline everywhere, simd128 where it validates WebAssembly SIMD. */
export function builds(): Build[] {
  return simdSupported() ? ["baseline", "simd128"] : ["baseline"];
}
