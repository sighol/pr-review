import { useEffect, useLayoutEffect, useRef } from "react";
import type { LineRange } from "../lib/diff";
import { langFor } from "../lib/highlight";
import { useStore, type Draft } from "../store";
import { Markdown } from "./Markdown";

interface Props {
  draft: Draft;
  /** Show a rendered preview under the textarea (Review tab). */
  preview?: boolean;
}

export function describeRange({ side, start, end }: LineRange) {
  return start === end ? `${side} line ${start}` : `${side} lines ${start}–${end}`;
}

export function CommentBox({ draft, preview = false }: Props) {
  const { focus, patchDraft, removeDraft, fileByName } = useStore();
  const focusTick = focus?.key === draft.key ? focus.tick : 0;
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
        <button className="link" onClick={() => removeDraft(draft.key)}>
          delete
        </button>
      </div>
      <textarea
        ref={ref}
        value={draft.text}
        placeholder="Leave a comment"
        onChange={(e) => patchDraft(draft.key, e.target.value)}
        onBlur={() => {
          if (draft.text.trim() === "" || draft.text === draft.initial) removeDraft(draft.key);
        }}
      />
      {preview && draft.text.trim() !== "" && (
        <div className="preview">
          <div className="preview-label">Preview</div>
          <Markdown text={draft.text} fileLang={langFor(fileByName.get(draft.file)?.path ?? "")} />
        </div>
      )}
    </div>
  );
}
