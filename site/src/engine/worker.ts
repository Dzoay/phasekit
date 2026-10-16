// The engine worker: the simd128 build where the browser validates SIMD, else the baseline, answering the page's
// requests off the main thread, so a long call never freezes the page.
import { createHandler } from "./handler";
import type { Request, Response } from "./protocol";
import { simdSupported } from "./simd";

interface Scope {
  onmessage: ((event: MessageEvent<Request>) => void) | null;
  postMessage(message: Response): void;
}

const scope = self as unknown as Scope;

const engine = (async () => {
  const simd = simdSupported();
  const pk = simd ? await import("../pk/simd/phasekit.js") : await import("../pk/baseline/phasekit.js");
  await pk.default();
  return createHandler(pk, simd ? "simd128" : "baseline");
})();

scope.onmessage = (event) => {
  const request = event.data;
  engine.then(
    (handle) => scope.postMessage(handle(request)),
    (e: unknown) =>
      scope.postMessage({
        id: request.id,
        ok: false,
        op: request.op,
        error: `the engine failed to load: ${String(e)}`,
      }),
  );
};
