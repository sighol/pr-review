import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import { suggestionBlock, textToLines, type LineRange } from "../lib/diff";
import { langFor } from "../lib/highlight";
import { insertBlock } from "../lib/suggest";
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
  const file = fileByName.get(draft.file);
  const focusTick = focus?.key === draft.key ? focus.tick : 0;
  const ref = useRef<HTMLTextAreaElement>(null);
  const pendingCaret = useRef<number | null>(null);
  const newLines = useMemo(() => textToLines(file?.newText ?? ""), [file?.newText]);
  const canSuggest = draft.range.side === "new";

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
    if (pendingCaret.current !== null) {
      el.focus();
      el.setSelectionRange(pendingCaret.current, pendingCaret.current);
      pendingCaret.current = null;
    }
  }, [draft.text]);

  useEffect(() => {
    const el = ref.current;
    if (!el || focusTick === 0) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [focusTick]);

  const suggest = () => {
    const el = ref.current;
    const caret = el ? el.selectionStart : draft.text.length;
    const next = insertBlock(draft.text, caret, suggestionBlock(draft.range, newLines));
    pendingCaret.current = next.caret;
    patchDraft(draft.key, next.text);
  };

  return (
    <div className="comment draft">
      <div className="comment-head">
        <span>Draft comment · {describeRange(draft.range)}</span>
        <span className="head-actions">
          {canSuggest && (
            <button
              className="action small"
              title="Insert a suggestion block with the current code"
              onMouseDown={(e) => e.preventDefault()}
              onClick={suggest}
            >
              Suggest
            </button>
          )}
          <button className="link" onClick={() => removeDraft(draft.key)}>
            delete
          </button>
        </span>
      </div>
      <textarea
        ref={ref}
        value={draft.text}
        placeholder="Leave a comment"
        onChange={(e) => patchDraft(draft.key, e.target.value)}
        onBlur={() => {
          if (draft.text.trim() === "") removeDraft(draft.key);
        }}
      />
      {preview && draft.text.trim() !== "" && (
        <div className="preview">
          <div className="preview-label">Preview</div>
          <Markdown
            text={draft.text}
            fileLang={langFor(file?.path ?? "")}
            replaces={canSuggest ? newLines.slice(draft.range.start - 1, draft.range.end) : null}
          />
        </div>
      )}
    </div>
  );
}
