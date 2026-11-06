import { useEffect, useState } from "react";
import { fileDiff, fileText } from "./backend";
import { stateOf, type CommitRange } from "./range";

export interface RangeData {
  /** Identifies the file and the two content states this data is for. */
  key: string;
  oldText: string;
  newText: string;
  diffText: string;
}

interface FileRef {
  path: string;
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
    fileText(file, from),
    fileText(file, to),
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
