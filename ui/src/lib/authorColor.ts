/** FNV-1a, 32 bit. */
function hash(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

// Fixed lightness and chroma keep every author equally bright and readable on the dark
// background (contrast above 8:1 on --bg); only the hue varies.
const LIGHTNESS = 0.78;
const CHROMA = 0.11;

const srgb = (c: number) => {
  const v = c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;
  return Math.round(Math.min(1, Math.max(0, v)) * 255);
};

/** OKLCH (hue in degrees) to a `#rrggbb` string. */
export function oklchToHex(l: number, c: number, hue: number): string {
  const a = c * Math.cos((hue * Math.PI) / 180);
  const b = c * Math.sin((hue * Math.PI) / 180);
  const l_ = Math.pow(l + 0.3963377774 * a + 0.2158037573 * b, 3);
  const m_ = Math.pow(l - 0.1055613458 * a - 0.0638541728 * b, 3);
  const s_ = Math.pow(l - 0.0894841775 * a - 1.291485548 * b, 3);
  const r = 4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_;
  const g = -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_;
  const bl = -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_;
  return "#" + [r, g, bl].map((x) => srgb(x).toString(16).padStart(2, "0")).join("");
}

/** A stable color for a person, identified by their (case-insensitive) login. */
export function authorColor(login: string): string {
  return oklchToHex(LIGHTNESS, CHROMA, hash(login.trim().toLowerCase()) % 360);
}
