import { describe, expect, test } from "bun:test";
import { authorColor, oklchToHex } from "./authorColor";

const luminance = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

describe("authorColor", () => {
  test("is deterministic and ignores case and spacing", () => {
    const a = authorColor("Alice");
    expect(a).toBe(authorColor(" alice "));
    expect(a).toMatch(/^#[0-9a-f]{6}$/);
  });

  test("different authors usually get different colors", () => {
    const colors = new Set(["alice", "bob", "carol", "dave", "erin"].map(authorColor));
    expect(colors.size).toBeGreaterThan(3);
  });

  test("every hue has good contrast on the app background and similar brightness", () => {
    const lums: number[] = [];
    for (let h = 0; h < 360; h += 5) {
      const hex = oklchToHex(0.78, 0.11, h);
      expect(contrast(hex, "#0d1117")).toBeGreaterThan(7);
      lums.push(luminance(hex));
    }
    expect(Math.max(...lums) - Math.min(...lums)).toBeLessThan(0.15);
  });
});
