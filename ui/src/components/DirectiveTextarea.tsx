import { Fragment, useMemo } from "react";
import { parseDirectives } from "../lib/directives";

interface Props {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

// A trailing newline needs a character after it before the backdrop gives it a line.
const ZERO_WIDTH_SPACE = String.fromCharCode(0x200b);

/**
 * A textarea whose directives (`+label`, `-label`, `+assignee:@user`) are highlighted. The
 * highlight is a div behind the transparent textarea, with the same text and font, so the box
 * grows with its content (both sit in one grid cell) and no scrolling has to be kept in sync.
 */
export function DirectiveTextarea({ value, onChange, placeholder }: Props) {
  const segments = useMemo(() => {
    const { spans } = parseDirectives(value);
    const out: { text: string; directive: boolean }[] = [];
    let at = 0;
    for (const s of spans) {
      if (s.start > at) out.push({ text: value.slice(at, s.start), directive: false });
      out.push({ text: value.slice(s.start, s.end), directive: true });
      at = s.end;
    }
    if (at < value.length) out.push({ text: value.slice(at), directive: false });
    return out;
  }, [value]);

  return (
    <div className="directive-editor">
      <div className="directive-backdrop" aria-hidden>
        {segments.map((s, i) => (
          <Fragment key={i}>{s.directive ? <mark>{s.text}</mark> : s.text}</Fragment>
        ))}
        {value.endsWith("\n") && ZERO_WIDTH_SPACE}
      </div>
      <textarea value={value} placeholder={placeholder} onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}
