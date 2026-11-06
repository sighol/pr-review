const CLOSE_LEN = "\n```".length;

/**
 * Inserts a fenced block at the caret on its own lines, and returns the caret
 * position at the end of the block's code (just before the closing fence).
 */
export function insertBlock(text: string, caret: number, block: string): { text: string; caret: number } {
  const before = text.slice(0, caret);
  const after = text.slice(caret);
  const lead = before !== "" && !before.endsWith("\n") ? "\n" : "";
  const trail = after !== "" && !after.startsWith("\n") ? "\n" : "";
  return {
    text: before + lead + block + trail + after,
    caret: before.length + lead.length + block.length - CLOSE_LEN,
  };
}
