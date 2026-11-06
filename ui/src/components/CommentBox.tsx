import { useEffect, useLayoutEffect, useRef } from "react";
import type { LineRange } from "../lib/diff";

export interface Draft {
  key: string;
  range: LineRange;
  text: string;
  initial: string;
}

interface Props {
  draft: Draft;
  focusTick: number;
  onChange: (key: string, text: string) => void;
  onDiscard: (key: string) => void;
}

export function describeRange({ side, start, end }: LineRange) {
  const where = side === "new" ? "new" : "old";
  return start === end ? `${where} line ${start}` : `${where} lines ${start}–${end}`;
}

export function CommentBox({ draft, focusTick, onChange, onDiscard }: Props) {
  const ref = useRef<HTMLTextAreaElement>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [draft.text]);

  useEffect(() => {
    const el = ref.current;
    if (!el || focusTick === 0) return;
    el.focus();
    const pos = draft.initial.startsWith("\n") ? 0 : el.value.length;
    el.setSelectionRange(pos, pos);
  }, [focusTick]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="comment draft">
      <div className="comment-head">
        <span>Draft comment · {describeRange(draft.range)}</span>
        <button className="link" onClick={() => onDiscard(draft.key)}>
          delete
        </button>
      </div>
      <textarea
        ref={ref}
        value={draft.text}
        placeholder="Leave a comment"
        onChange={(e) => onChange(draft.key, e.target.value)}
        onBlur={() => {
          if (draft.text.trim() === "" || draft.text === draft.initial) onDiscard(draft.key);
        }}
      />
    </div>
  );
}
