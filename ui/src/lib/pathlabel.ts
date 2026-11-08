/**
 * Shortens a long path for display: while it is still longer than `maxLen`, the middle-most
 * directory that isn't already a single letter is collapsed to its first letter. The first
 * directory and the filename are never touched, so e.g. "api/alpha/bravo/charlie/delta/MyFile.kt"
 * becomes "api/a/b/c/d/MyFile.kt". A path with nothing between the first directory and the
 * filename is returned unchanged.
 */
export function compressPath(path: string, maxLen = 60): string {
  const parts = path.split("/");
  if (parts.length <= 2) return path;

  while (parts.join("/").length > maxLen) {
    const shrinkable = parts.slice(1, -1).reduce<number[]>((acc, p, i) => {
      if (p.length > 1) acc.push(i + 1);
      return acc;
    }, []);
    if (shrinkable.length === 0) break;
    const target = shrinkable[Math.floor(shrinkable.length / 2)];
    parts[target] = parts[target][0];
  }
  return parts.join("/");
}
