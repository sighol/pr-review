import { useEffect, useState } from "react";
import { copyText } from "../lib/clipboard";

/** A small button that copies `text` and says so for a moment. */
export function CopyButton({ text, title }: { text: string; title: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

  useEffect(() => {
    if (state === "idle") return;
    const timer = setTimeout(() => setState("idle"), 1500);
    return () => clearTimeout(timer);
  }, [state]);

  return (
    <button
      className="action small copy-button"
      title={title}
      onClick={async () => setState((await copyText(text)) ? "copied" : "failed")}
    >
      {state === "copied" ? "Copied ✓" : state === "failed" ? "Copy failed" : "Copy"}
    </button>
  );
}
