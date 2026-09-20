// The device's 128×64 one-bit display images: packing, drawing and image conversion.

export const WIDTH = 128;
export const HEIGHT = 64;
/** Bytes per packed row; the leftmost pixel of a byte is its high bit. */
export const STRIDE = WIDTH / 8;
export const ROWS_LEN = STRIDE * HEIGHT;

/** One byte per pixel, row-major: 1 is lit, 0 is dark. */
export type Pixels = Uint8Array;

export const STARTUP_SLOTS = ["startup_1", "startup_2"];
export const SCREEN_SAVER_SLOTS = ["screen_saver_1", "screen_saver_2", "screen_saver_3", "screen_saver_4"];

export function blank(): Pixels {
  return new Uint8Array(WIDTH * HEIGHT);
}

/** Expand the packed rows the backend sends. */
export function unpack(rows: ArrayLike<number>): Pixels {
  const px = blank();
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      px[y * WIDTH + x] = (rows[y * STRIDE + (x >> 3)] >> (7 - (x & 7))) & 1;
    }
  }
  return px;
}

/** Pack for the backend, which expects a plain array over the IPC bridge. */
export function pack(px: Pixels): number[] {
  const rows = new Array<number>(ROWS_LEN).fill(0);
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      if (px[y * WIDTH + x]) rows[y * STRIDE + (x >> 3)] |= 1 << (7 - (x & 7));
    }
  }
  return rows;
}

export function samePixels(a: Pixels, b: Pixels): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

export function invert(px: Pixels): Pixels {
  const out = new Uint8Array(px.length);
  for (let i = 0; i < px.length; i++) out[i] = px[i] ? 0 : 1;
  return out;
}

/** Paint an image onto a canvas sized WIDTH × HEIGHT, in the device's colours. */
export function draw(canvas: HTMLCanvasElement, px: Pixels) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const image = ctx.createImageData(WIDTH, HEIGHT);
  for (let i = 0; i < px.length; i++) {
    // The device's display is light on dark; the accent colour reads as its backlight.
    const [r, g, b] = px[i] ? [255, 173, 51] : [26, 20, 10];
    image.data[i * 4] = r;
    image.data[i * 4 + 1] = g;
    image.data[i * 4 + 2] = b;
    image.data[i * 4 + 3] = 255;
  }
  ctx.putImageData(image, 0, 0);
}

export type Fit = "contain" | "cover" | "stretch";
export type Dither = "none" | "floyd" | "atkinson" | "bayer4" | "bayer8";

export interface Conversion {
  fit: Fit;
  /** −100…100. */
  brightness: number;
  /** −100…100. */
  contrast: number;
  /** 0…255: the grey level that becomes a lit pixel. */
  threshold: number;
  dither: Dither;
  invert: boolean;
}

export const DEFAULT_CONVERSION: Conversion = {
  fit: "contain",
  brightness: 0,
  contrast: 0,
  threshold: 128,
  dither: "floyd",
  invert: false,
};

const BAYER_2 = [
  [0, 2],
  [3, 1],
];

/** Bayer matrices double by the usual recursion; values are normalised to 0…1. */
function bayer(size: number): number[][] {
  let m = BAYER_2;
  while (m.length < size) {
    const n = m.length;
    const next = Array.from({ length: n * 2 }, () => new Array<number>(n * 2).fill(0));
    for (let y = 0; y < n; y++) {
      for (let x = 0; x < n; x++) {
        next[y][x] = m[y][x] * 4;
        next[y][x + n] = m[y][x] * 4 + 2;
        next[y + n][x] = m[y][x] * 4 + 3;
        next[y + n][x + n] = m[y][x] * 4 + 1;
      }
    }
    m = next;
  }
  const scale = size * size;
  return m.map((row) => row.map((v) => (v + 0.5) / scale));
}

const BAYER_4 = bayer(4);
const BAYER_8 = bayer(8);

/** Grey levels 0…255 for an image scaled into the display, composited over black. */
function greyscale(source: CanvasImageSource, sw: number, sh: number, fit: Fit): Float32Array {
  const canvas = document.createElement("canvas");
  canvas.width = WIDTH;
  canvas.height = HEIGHT;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return new Float32Array(WIDTH * HEIGHT);
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, WIDTH, HEIGHT);
  if (fit === "stretch") {
    ctx.drawImage(source, 0, 0, WIDTH, HEIGHT);
  } else {
    const scale = fit === "cover" ? Math.max(WIDTH / sw, HEIGHT / sh) : Math.min(WIDTH / sw, HEIGHT / sh);
    const w = sw * scale;
    const h = sh * scale;
    ctx.drawImage(source, (WIDTH - w) / 2, (HEIGHT - h) / 2, w, h);
  }
  const data = ctx.getImageData(0, 0, WIDTH, HEIGHT).data;
  const grey = new Float32Array(WIDTH * HEIGHT);
  for (let i = 0; i < grey.length; i++) {
    grey[i] = 0.2126 * data[i * 4] + 0.7152 * data[i * 4 + 1] + 0.0722 * data[i * 4 + 2];
  }
  return grey;
}

function adjust(grey: Float32Array, brightness: number, contrast: number) {
  const c = contrast * 2.55;
  const factor = (259 * (c + 255)) / (255 * (259 - c));
  const offset = brightness * 2.55;
  for (let i = 0; i < grey.length; i++) {
    grey[i] = factor * (grey[i] + offset - 128) + 128;
  }
}

/** Error-diffusion weights: offset x, offset y, share of the error. */
const FLOYD: [number, number, number][] = [
  [1, 0, 7 / 16],
  [-1, 1, 3 / 16],
  [0, 1, 5 / 16],
  [1, 1, 1 / 16],
];
const ATKINSON: [number, number, number][] = [
  [1, 0, 1 / 8],
  [2, 0, 1 / 8],
  [-1, 1, 1 / 8],
  [0, 1, 1 / 8],
  [1, 1, 1 / 8],
  [0, 2, 1 / 8],
];

function diffuse(grey: Float32Array, threshold: number, weights: [number, number, number][]): Pixels {
  const px = blank();
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const i = y * WIDTH + x;
      const lit = grey[i] >= threshold;
      px[i] = lit ? 1 : 0;
      const error = grey[i] - (lit ? 255 : 0);
      for (const [dx, dy, share] of weights) {
        const nx = x + dx;
        const ny = y + dy;
        if (nx < 0 || nx >= WIDTH || ny >= HEIGHT) continue;
        grey[ny * WIDTH + nx] += error * share;
      }
    }
  }
  return px;
}

function ordered(grey: Float32Array, threshold: number, matrix: number[][]): Pixels {
  const px = blank();
  const n = matrix.length;
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      // Shift the decision point around the threshold by where the pixel sits in the matrix.
      const bias = (matrix[y % n][x % n] - 0.5) * 255;
      px[y * WIDTH + x] = grey[y * WIDTH + x] >= threshold + bias ? 1 : 0;
    }
  }
  return px;
}

/** Convert a loaded image to the display's one-bit pixels. */
export function convert(source: CanvasImageSource, sw: number, sh: number, c: Conversion): Pixels {
  const grey = greyscale(source, sw, sh, c.fit);
  adjust(grey, c.brightness, c.contrast);
  let px: Pixels;
  switch (c.dither) {
    case "floyd":
      px = diffuse(grey, c.threshold, FLOYD);
      break;
    case "atkinson":
      px = diffuse(grey, c.threshold, ATKINSON);
      break;
    case "bayer4":
      px = ordered(grey, c.threshold, BAYER_4);
      break;
    case "bayer8":
      px = ordered(grey, c.threshold, BAYER_8);
      break;
    default:
      px = ordered(grey, c.threshold, [[0.5]]);
  }
  return c.invert ? invert(px) : px;
}

/** Load a picked file into something the canvas can draw. */
export async function loadImage(file: File): Promise<{ source: CanvasImageSource; width: number; height: number }> {
  const url = URL.createObjectURL(file);
  try {
    const img = new Image();
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error(`${file.name} is not an image this app can read`));
      img.src = url;
    });
    // Decoding is done by the time onload fires, so the object URL can go.
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    canvas.getContext("2d")?.drawImage(img, 0, 0);
    return { source: canvas, width: img.naturalWidth, height: img.naturalHeight };
  } finally {
    URL.revokeObjectURL(url);
  }
}
