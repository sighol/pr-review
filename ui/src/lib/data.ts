import { useEffect, useState } from "react";
import { mapLine, needsAnchor, type ThreadAnchors } from "./anchor";
import { fileDiff, fileText } from "./backend";
import { textToLines } from "./diff";
import { stateOf, type CommitRange } from "./range";
import type { FakeThread } from "./threads";

export interface RangeData {
  /** Identifies the file and the two content states this data is for. */
  key: string;
  oldText: string;
  newText: string;
  diffText: string;
}

interface FileRef {
  path: string;
  oldPath?: string;
  dir: string;
  changedBy: readonly number[];
}

export const rangeKey = (file: FileRef, range: CommitRange) =>
  `${file.path}:${stateOf(file.changedBy, range.from)}-${stateOf(file.changedBy, range.to)}`;

/** Loads the old/new text and git diff for a commit range of one file. */
export async function loadRangeData(file: FileRef, range: CommitRange): Promise<RangeData> {
  const from = stateOf(file.changedBy, range.from);
  const to = stateOf(file.changedBy, range.to);
  const [oldText, newText, diffText] = await Promise.all([
    fileText(file, from, range.to),
    fileText(file, to, range.to),
    from === to ? Promise.resolve("") : fileDiff(file, from, to),
  ]);
  return { key: rangeKey(file, range), oldText, newText, diffText };
}

/** The data for a range; keeps showing the previous range of the same file while the next one loads. */
export function useRangeData(file: FileRef | undefined, range: CommitRange | undefined): RangeData | null {
  const [data, setData] = useState<RangeData | null>(null);
  const key = file && range ? rangeKey(file, range) : null;

  useEffect(() => {
    if (!file || !range) return;
    let cancelled = false;
    loadRangeData(file, range).then((d) => !cancelled && setData(d));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return data && file && data.key.startsWith(`${file.path}:`) ? data : null;
}

/**
 * Closest-match lines for the file's outdated threads that this range can't place exactly, mapped
 * from the version each was written on to the range's new text. Null until loaded for `data`.
 */
export function useThreadAnchors(
  file: FileRef | undefined,
  range: CommitRange | undefined,
  data: RangeData | null,
  threads: readonly FakeThread[],
): ThreadAnchors | null {
  const [result, setResult] = useState<{ key: string; anchors: ThreadAnchors } | null>(null);
  const wanted =
    file && range && data
      ? threads.filter((t) => t.file === file.path && needsAnchor(t, range, file.changedBy))
      : [];
  const key = data ? `${data.key}|${wanted.map((t) => `${t.id}@${t.commit}:${t.originalLine}`).join(",")}` : null;

  useEffect(() => {
    if (!file || !data || key === null) return;
    let cancelled = false;
    const newLines = textToLines(data.newText);
    Promise.all(
      wanted.map(async (t) => {
        const text = await fileText(file, stateOf(file.changedBy, t.commit), t.commit);
        return [t.id, mapLine(textToLines(text), newLines, t.originalLine).line] as const;
      }),
    )
      .catch(() => [] as (readonly [string, number])[])
      .then((pairs) => !cancelled && setResult({ key, anchors: new Map(pairs) }));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return result && result.key === key ? result.anchors : null;
}
