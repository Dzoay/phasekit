// The surfaces' WebGL scene (PLAN.md M26.2; user decision DG7): three.js draws the mesh the page placed in its cube
// (mesh.ts), lit so that its folds show, each triangle one phase's colour, the dome as a line over it, the critical
// point as a marker, and three of the cube's edges as axes with their values; turned and zoomed with the mouse or a
// finger (OrbitControls), and read under the cursor, the triangle and the point in it handed to the page. It draws
// only when something changed. Browser-only glue, which Playwright drives (tests/e2e/surfaces.spec.ts);
// scripts/site/mutants.sh does not mutate it.
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
  Raycaster,
  SRGBColorSpace,
  Scene,
  Triangle,
  Vector2,
  Vector3,
  WebGLRenderer,
  type Material,
  type Object3D,
} from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { CSS2DObject, CSS2DRenderer } from "three/addons/renderers/CSS2DRenderer.js";

/** An axis as drawn: its title and its values, each at its place along the axis, −1 to 1. */
export interface DrawnAxis {
  title: string;
  ticks: { at: number; label: string }[];
}

/** What the scene draws: the vertices' places, the triangles and each one's colour, the dome's path, the critical
 *  point, the axes (across, up, deep) and the CSS colours of the lines and the marker. */
export interface Drawn {
  xyz: Float32Array;
  triangles: Uint32Array;
  colours: Float32Array;
  dome: number[];
  critical: number | null;
  axes: DrawnAxis[];
  line: string;
  box: string;
  marker: string;
}

/** A point under the cursor: its triangle and its barycentric weights there. */
export interface Pick {
  triangle: number;
  weights: [number, number, number];
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

/** The surface as triangles of their own, each its one colour, the normals smoothed across the shared vertices first so
 *  that the lighting stays smooth where the colour does not. */
function surface(d: Drawn): BufferGeometry {
  const shared = geometry(d.xyz).setIndex(new BufferAttribute(d.triangles, 1));
  shared.computeVertexNormals();
  const own = shared.toNonIndexed();
  shared.dispose();
  const colours = new Float32Array(3 * own.getAttribute("position").count);
  for (let t = 0; t < d.colours.length / 3; t++) {
    const rgb = d.colours.subarray(3 * t, 3 * t + 3);
    for (let k = 0; k < 3; k++) colours.set(rgb, 9 * t + 3 * k);
  }
  return own.setAttribute("color", new BufferAttribute(colours, 3));
}

/** Where each axis runs along the cube: across on the near bottom edge, up on the near left one, deep on the bottom
 *  right one; and which way its labels stand off the edge. */
const EDGES: readonly { at: (s: number) => Vector3; off: Vector3 }[] = [
  { at: (s) => new Vector3(s, -1, 1), off: new Vector3(0, -1, 1) },
  { at: (s) => new Vector3(-1, s, 1), off: new Vector3(-1, 0, 1) },
  { at: (s) => new Vector3(1, -1, -s), off: new Vector3(1, -1, 0) },
];

function label(text: string, className: string, at: Vector3): CSS2DObject {
  const element = document.createElement("span");
  element.className = className;
  element.textContent = text;
  const object = new CSS2DObject(element);
  object.position.copy(at);
  return object;
}

export class SurfaceScene {
  readonly #renderer: WebGLRenderer;
  readonly #labels = new CSS2DRenderer();
  readonly #scene = new Scene();
  readonly #camera = new PerspectiveCamera(32, 1, 0.1, 100);
  readonly #controls: OrbitControls;
  readonly #raycaster = new Raycaster();
  readonly #canvas: HTMLCanvasElement;
  #drawn: Object3D[] = [];
  #surface: Mesh | null = null;
  #marker: Points | null = null;
  #dragging = false;
  #pending: PointerEvent | null = null;

  /** The scene on a canvas, its labels in a layer over it in `host`; `pick` hears the point under the cursor, or null
   *  where there is none. */
  constructor(canvas: HTMLCanvasElement, host: HTMLElement, pick: (p: Pick | null) => void) {
    this.#canvas = canvas;
    // The drawing buffer is kept so that the page can save the view as a PNG.
    this.#renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true });
    const layer = this.#labels.domElement;
    Object.assign(layer.style, { position: "absolute", inset: "0", pointerEvents: "none" });
    layer.className = "labels";
    host.append(layer);
    this.#camera.position.set(3.0, 2.4, 6.1);
    this.#camera.lookAt(0, 0, 0);
    const light = new DirectionalLight(0xffffff, 1.8);
    light.position.set(1, 4, 3);
    this.#scene.add(new AmbientLight(0xffffff, 1.1), light);
    this.#controls = new OrbitControls(this.#camera, canvas);
    this.#controls.addEventListener("change", () => this.render());
    this.#controls.addEventListener("start", () => (this.#dragging = true));
    this.#controls.addEventListener("end", () => (this.#dragging = false));
    canvas.addEventListener("pointermove", (e) => this.#hover(e, pick));
    canvas.addEventListener("pointerleave", () => {
      this.#pending = null;
      this.#mark(null);
      pick(null);
    });
  }

  /** Replaces what is drawn. */
  draw(d: Drawn): void {
    this.#clear();
    const mesh = new Mesh(surface(d), new MeshLambertMaterial({ vertexColors: true, side: DoubleSide }));
    const dome = new Line(geometry(along(d.xyz, d.dome)), new LineBasicMaterial({ color: d.line }));
    const critical = new Points(
      geometry(along(d.xyz, d.critical === null ? [] : [d.critical])),
      new PointsMaterial({ color: d.line, size: 9, sizeAttenuation: false }),
    );
    const box = new LineSegments(new EdgesGeometry(new BoxGeometry(2, 2, 2)), new LineBasicMaterial({ color: d.box }));
    const marker = new Points(
      geometry(new Float32Array(3)),
      new PointsMaterial({ color: d.marker, size: 11, sizeAttenuation: false }),
    );
    marker.visible = false;
    [this.#surface, this.#marker] = [mesh, marker];
    this.#drawn = [mesh, dome, critical, box, marker, ...this.#axes(d.axes, d.line)];
    this.#scene.add(...this.#drawn);
    this.render();
  }

  /** The canvas's size in CSS pixels. */
  resize(width: number, height: number): void {
    this.#renderer.setPixelRatio(window.devicePixelRatio);
    this.#renderer.setSize(width, height, false);
    this.#labels.setSize(width, height);
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
    this.#labels.render(this.#scene, this.#camera);
  }

  dispose(): void {
    this.#clear();
    this.#controls.dispose();
    this.#renderer.dispose();
    this.#labels.domElement.remove();
  }

  /** Each axis's ticks, a short stroke off its edge, and its values and title as labels beyond them. */
  #axes(axes: DrawnAxis[], colour: string): Object3D[] {
    const strokes: number[] = [];
    const labels: Object3D[] = [];
    axes.forEach((axis, k) => {
      const edge = EDGES[k];
      if (!edge) return;
      for (const tick of axis.ticks) {
        const at = edge.at(tick.at);
        strokes.push(...at.toArray(), ...at.clone().addScaledVector(edge.off, 0.05).toArray());
        labels.push(label(tick.label, "tick", at.clone().addScaledVector(edge.off, 0.13)));
      }
      labels.push(label(axis.title, "title", edge.at(0).addScaledVector(edge.off, 0.34)));
    });
    const ticks = new LineSegments(geometry(new Float32Array(strokes)), new LineBasicMaterial({ color: colour }));
    return [ticks, ...labels];
  }

  // At most one pick a frame, and none while the view is being turned.
  #hover(e: PointerEvent, pick: (p: Pick | null) => void): void {
    if (this.#dragging) return;
    const first = this.#pending === null;
    this.#pending = e;
    if (!first) return;
    requestAnimationFrame(() => {
      const event = this.#pending;
      this.#pending = null;
      if (event) pick(this.#pick(event));
    });
  }

  #pick(e: PointerEvent): Pick | null {
    const box = this.#canvas.getBoundingClientRect();
    const ndc = new Vector2(
      ((e.clientX - box.left) / box.width) * 2 - 1,
      -((e.clientY - box.top) / box.height) * 2 + 1,
    );
    this.#raycaster.setFromCamera(ndc, this.#camera);
    const mesh = this.#surface;
    const hit = mesh ? this.#raycaster.intersectObject(mesh)[0] : undefined;
    const face = hit?.faceIndex;
    if (!mesh || !hit || face === undefined || face === null) {
      this.#mark(null);
      return null;
    }
    const position = mesh.geometry.getAttribute("position");
    const corner = (k: number) => new Vector3().fromBufferAttribute(position, 3 * face + k);
    const weights = Triangle.getBarycoord(hit.point, corner(0), corner(1), corner(2), new Vector3());
    if (!weights) return null;
    this.#mark(hit.point);
    return { triangle: face, weights: [weights.x, weights.y, weights.z] };
  }

  #mark(at: Vector3 | null): void {
    if (!this.#marker) return;
    const position = this.#marker.geometry.getAttribute("position");
    this.#marker.visible = at !== null;
    if (at) position.setXYZ(0, at.x, at.y, at.z);
    position.needsUpdate = true;
    this.render();
  }

  #clear(): void {
    for (const object of this.#drawn) {
      this.#scene.remove(object);
      if (object instanceof CSS2DObject) {
        object.element.remove();
        continue;
      }
      const { geometry, material } = object as Object3D & { geometry: BufferGeometry; material: Material };
      geometry.dispose();
      material.dispose();
    }
    [this.#drawn, this.#surface, this.#marker] = [[], null, null];
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
