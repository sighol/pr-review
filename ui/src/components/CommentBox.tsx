import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import { suggestionBlock, textToLines, type LineRange } from "../lib/diff";
import { langFor } from "../lib/highlight";
import { insertBlock } from "../lib/suggest";
import { useStore, type Draft } from "../store";
import { Markdown } from "./Markdown";

export type BoxMode = "write" | "preview";

interface Props {
  draft: Draft;
  /** Shared write/preview mode for every draft box on the Review tab; omit to always show the textarea (DiffView). */
  mode?: BoxMode;
  onModeChange?: (mode: BoxMode) => void;
}

export function describeRange({ side, start, end }: LineRange) {
  return start === end ? `${side} line ${start}` : `${side} lines ${start}–${end}`;
}

export function CommentBox({ draft, mode, onModeChange }: Props) {
  const { focus, patchDraft, removeDraft, fileByName } = useStore();
  const file = fileByName.get(draft.file);
  const focusTick = focus?.key === draft.key ? focus.tick : 0;
  const ref = useRef<HTMLTextAreaElement>(null);
  const pendingCaret = useRef<number | null>(null);
  const newLines = useMemo(() => textToLines(file?.newText ?? ""), [file?.newText]);
  const canSuggest = draft.range.side === "new";
  const showToggle = mode !== undefined;
  const effectiveMode = mode ?? "write";

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
    onModeChange?.("write");
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [focusTick]);

  const suggest = () => {
    const el = ref.current;
    const caret = el ? el.selectionStart : draft.text.length;
    const next = insertBlock(draft.text, caret, suggestionBlock(draft.range, newLines));
    pendingCaret.current = next.caret;
    onModeChange?.("write");
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
      {showToggle && (
        <div className="mode-toggle">
          <button
            className={`mode-tab${effectiveMode === "write" ? " active" : ""}`}
            onClick={() => onModeChange?.("write")}
          >
            Write
          </button>
          <button
            className={`mode-tab${effectiveMode === "preview" ? " active" : ""}`}
            onClick={() => onModeChange?.("preview")}
          >
            Preview
          </button>
        </div>
      )}
      <textarea
        ref={ref}
        value={draft.text}
        placeholder="Leave a comment"
        onChange={(e) => patchDraft(draft.key, e.target.value)}
        onBlur={() => {
          if (draft.text.trim() === "") removeDraft(draft.key);
        }}
        style={showToggle && effectiveMode === "preview" ? { display: "none" } : undefined}
      />
      {showToggle && effectiveMode === "preview" && (
        <div className="preview">
          {draft.text.trim() !== "" ? (
            <Markdown
              text={draft.text}
              fileLang={langFor(file?.path ?? "")}
              replaces={canSuggest ? newLines.slice(draft.range.start - 1, draft.range.end) : null}
            />
          ) : (
            <div className="muted">Nothing to preview yet.</div>
          )}
        </div>
      )}
    </div>
  );
}
