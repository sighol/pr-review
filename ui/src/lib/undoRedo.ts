import type { KeyboardEvent } from "react";
import { inTauri } from "./backend";

/**
 * Workaround for https://github.com/tauri-apps/tauri/issues/16299: in Tauri's WebKitGTK webview,
 * Ctrl/Cmd+Z and Ctrl/Cmd+Shift+Z (or Ctrl+Y) on a textarea do nothing, because the webview never
 * runs the browser's native undo/redo. `document.execCommand("undo"/"redo")` still does, so this
 * dispatches it by hand. Only applied inside Tauri; browsers already handle this natively.
 */
export function undoRedoWorkaround(e: KeyboardEvent<HTMLTextAreaElement>) {
  if (!inTauri()) return;
  const mod = e.metaKey || e.ctrlKey;
  if (!mod) return;
  const key = e.key.toLowerCase();
  if (key === "z" && !e.shiftKey) {
    e.preventDefault();
    document.execCommand("undo");
  } else if ((key === "z" && e.shiftKey) || key === "y") {
    e.preventDefault();
    document.execCommand("redo");
  }
}
