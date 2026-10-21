// The surfaces' WebGL scene (PLAN.md M26.2; user decision DG7): three.js draws the mesh the page placed in its cube
// (mesh.ts), coloured by phase and lit so that its folds show, the dome as a line over it, the critical point as a
// marker and the cube's edges, turned and zoomed with the mouse or a finger (OrbitControls). It draws only when
// something changed. Browser-only glue, which Playwright drives (tests/e2e/surfaces.spec.ts); scripts/site/mutants.sh
// does not mutate it.
import {
  AmbientLight,
  BoxGeometry,
  BufferAttribute,
  BufferGeometry,
  Color,
  DirectionalLight,
  DoubleSide,
  EdgesGeometry,
  Line,
  LineBasicMaterial,
  LineSegments,
  Mesh,
  MeshLambertMaterial,
  PerspectiveCamera,
  Points,
  PointsMaterial,
  SRGBColorSpace,
  Scene,
  WebGLRenderer,
  type Material,
  type Object3D,
} from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";

/** What the scene draws: the vertices' places and colours, the triangles, the dome's path and the critical point. */
export interface Drawn {
  xyz: Float32Array;
  colours: Float32Array;
  triangles: Uint32Array;
  dome: number[];
  critical: number | null;
  /** CSS colours of the dome, the critical point and the cube's edges. */
  line: string;
  box: string;
}

/** The vertices' places a path visits, three a point. */
function along(xyz: Float32Array, path: number[]): Float32Array {
  const out = new Float32Array(3 * path.length);
  path.forEach((i, k) => out.set(xyz.subarray(3 * i, 3 * i + 3), 3 * k));
  return out;
}

function geometry(xyz: Float32Array): BufferGeometry {
  return new BufferGeometry().setAttribute("position", new BufferAttribute(xyz, 3));
}

export class SurfaceScene {
  readonly #renderer: WebGLRenderer;
  readonly #scene = new Scene();
  readonly #camera = new PerspectiveCamera(32, 1, 0.1, 100);
  readonly #controls: OrbitControls;
  #drawn: Object3D[] = [];

  constructor(canvas: HTMLCanvasElement) {
    // The drawing buffer is kept so that the page can save the view as a PNG.
    this.#renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true });
    this.#camera.position.set(2.6, 2.1, 5.3);
    this.#camera.lookAt(0, 0, 0);
    const light = new DirectionalLight(0xffffff, 1.8);
    light.position.set(1, 4, 3);
    this.#scene.add(new AmbientLight(0xffffff, 1.1), light);
    this.#controls = new OrbitControls(this.#camera, canvas);
    this.#controls.addEventListener("change", () => this.render());
  }

  /** Replaces what is drawn. */
  draw(d: Drawn): void {
    this.#clear();
    const surface = geometry(d.xyz).setAttribute("color", new BufferAttribute(d.colours, 3));
    surface.setIndex(new BufferAttribute(d.triangles, 1));
    surface.computeVertexNormals();
    const material = new MeshLambertMaterial({ vertexColors: true, side: DoubleSide });
    const dome = new Line(geometry(along(d.xyz, d.dome)), new LineBasicMaterial({ color: d.line }));
    const critical = new Points(
      geometry(along(d.xyz, d.critical === null ? [] : [d.critical])),
      new PointsMaterial({ color: d.line, size: 9, sizeAttenuation: false }),
    );
    const box = new LineSegments(new EdgesGeometry(new BoxGeometry(2, 2, 2)), new LineBasicMaterial({ color: d.box }));
    this.#drawn = [new Mesh(surface, material), dome, critical, box];
    this.#scene.add(...this.#drawn);
    this.render();
  }

  /** The canvas's size in CSS pixels. */
  resize(width: number, height: number): void {
    this.#renderer.setPixelRatio(window.devicePixelRatio);
    this.#renderer.setSize(width, height, false);
    this.#camera.aspect = width / height;
    this.#camera.updateProjectionMatrix();
    this.render();
  }

  /** The view as it was first shown. */
  reset(): void {
    this.#controls.reset();
  }

  render(): void {
    this.#renderer.render(this.#scene, this.#camera);
  }

  dispose(): void {
    this.#clear();
    this.#controls.dispose();
    this.#renderer.dispose();
  }

  #clear(): void {
    for (const object of this.#drawn) {
      this.#scene.remove(object);
      const { geometry, material } = object as Object3D & { geometry: BufferGeometry; material: Material };
      geometry.dispose();
      material.dispose();
    }
    this.#drawn = [];
  }
}

/** A CSS colour's channels, linear as three.js lights them, from the sRGB the browser resolves it to (a token may be
 *  oklch or color-mix, which three.js does not read). */
export function linearRgb(css: string): [number, number, number] {
  const context = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  if (!context) return [0.5, 0.5, 0.5];
  context.fillStyle = css;
  context.fillRect(0, 0, 1, 1);
  const [r = 0, g = 0, b = 0] = context.getImageData(0, 0, 1, 1).data;
  const c = new Color().setRGB(r / 255, g / 255, b / 255, SRGBColorSpace);
  return [c.r, c.g, c.b];
}
