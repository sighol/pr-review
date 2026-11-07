/**
 * Directives in the review body: `+label`, `-label`, `+"label with spaces"`, `+assignee:@user`,
 * `-assignee:@user`. Whole whitespace-delimited tokens only, a sign followed by a letter (or a
 * quote), and never inside code. They are removed from the text that is posted.
 */

export interface Directives {
  addLabels: string[];
  removeLabels: string[];
  addAssignees: string[];
  removeAssignees: string[];
}

export interface DirectiveSpan {
  start: number;
  end: number;
}

export interface Parsed {
  /** The body without the directives, trimmed. */
  text: string;
  directives: Directives;
  /** Where the directives are in the original body, for highlighting. */
  spans: DirectiveSpan[];
}

export const noDirectives = (): Directives => ({
  addLabels: [],
  removeLabels: [],
  addAssignees: [],
  removeAssignees: [],
});

export const hasDirectives = (d: Directives) =>
  d.addLabels.length + d.removeLabels.length + d.addAssignees.length + d.removeAssignees.length > 0;

const ASSIGNEE = "assignee:";
// Stands in for code so the token regex cannot match inside it; never a letter, quote or space.
const MASK = "\u0001";

/** The body with fenced code blocks and inline code replaced by MASK, keeping offsets. */
function maskCode(body: string): string {
  let fence: string | null = null;
  const lines = body.split("\n").map((line) => {
    const m = /^ {0,3}(`{3,}|~{3,})/.exec(line);
    if (fence === null) {
      if (m) {
        fence = m[1][0];
        return MASK.repeat(line.length);
      }
      return line.replace(/(`+)(?:(?!\1)[^\n])+?\1/g, (s) => MASK.repeat(s.length));
    }
    if (m && m[1][0] === fence) fence = null;
    return MASK.repeat(line.length);
  });
  return lines.join("\n");
}

const TOKEN = /(?<=^|\s)([+-])(?:"([^"\n]+)"(?=\s|$)|([A-Za-z]\S*))/g;

interface Token extends DirectiveSpan {
  sign: "+" | "-";
  name: string;
  quoted: boolean;
}

function findTokens(body: string): Token[] {
  const masked = maskCode(body);
  const out: Token[] = [];
  for (const m of masked.matchAll(TOKEN)) {
    const start = m.index!;
    const end = start + m[0].length;
    const quoted = m[2] !== undefined;
    const name = quoted ? body.slice(start + 2, end - 1) : body.slice(start + 1, end);
    out.push({ start, end, sign: m[1] as "+" | "-", name, quoted });
  }
  return out;
}

const pushOnce = (list: string[], s: string) => {
  if (!list.some((x) => x.toLowerCase() === s.toLowerCase())) list.push(s);
};

/** Removes the tokens from one line along with the space that separated them from the text. */
function stripLine(line: string, tokens: DirectiveSpan[]): string {
  let out = line;
  for (const t of [...tokens].reverse()) {
    let { start, end } = t;
    while (start > 0 && (out[start - 1] === " " || out[start - 1] === "\t")) start--;
    if (start === 0) while (out[end] === " " || out[end] === "\t") end++;
    out = out.slice(0, start) + out.slice(end);
  }
  return out.replace(/[ \t]+$/, "");
}

export function parseDirectives(body: string): Parsed {
  const tokens = findTokens(body);
  const directives = noDirectives();
  for (const t of tokens) {
    if (!t.quoted && t.name.toLowerCase().startsWith(ASSIGNEE)) {
      const user = t.name.slice(ASSIGNEE.length).replace(/^@/, "");
      pushOnce(t.sign === "+" ? directives.addAssignees : directives.removeAssignees, user);
    } else {
      pushOnce(t.sign === "+" ? directives.addLabels : directives.removeLabels, t.name);
    }
  }
  if (tokens.length === 0) return { text: body.trim(), directives, spans: [] };

  let offset = 0;
  const kept: string[] = [];
  for (const line of body.split("\n")) {
    const mine = tokens
      .filter((t) => t.start >= offset && t.end <= offset + line.length)
      .map((t) => ({ start: t.start - offset, end: t.end - offset }));
    offset += line.length + 1;
    if (mine.length === 0) {
      kept.push(line);
      continue;
    }
    const stripped = stripLine(line, mine);
    if (stripped.trim() !== "") kept.push(stripped);
  }
  return {
    text: kept.join("\n").trim(),
    directives,
    spans: tokens.map(({ start, end }) => ({ start, end })),
  };
}

const quoteIfNeeded = (name: string) => (/^[A-Za-z]\S*$/.test(name) ? name : `"${name}"`);

/** Writes directives back in the syntax `parseDirectives` reads. */
export function formatDirectives(d: Directives): string {
  const parts = [
    ...d.addLabels.map((l) => `+${quoteIfNeeded(l)}`),
    ...d.removeLabels.map((l) => `-${quoteIfNeeded(l)}`),
    ...d.addAssignees.map((u) => `+${ASSIGNEE}@${u}`),
    ...d.removeAssignees.map((u) => `-${ASSIGNEE}@${u}`),
  ];
  return parts.join(" ");
}

/** What will happen, in words, for the line under the editor. */
export function describeDirectives(d: Directives, hasText: boolean): string[] {
  const out: string[] = [];
  if (d.removeLabels.length) out.push(`Remove: ${d.removeLabels.join(", ")}`);
  if (d.addLabels.length) out.push(`Add: ${d.addLabels.join(", ")}`);
  if (d.addAssignees.length) out.push(`Assign: ${d.addAssignees.map((u) => `@${u}`).join(", ")}`);
  if (d.removeAssignees.length) out.push(`Unassign: ${d.removeAssignees.map((u) => `@${u}`).join(", ")}`);
  if (!hasText && out.length) out.push("no comment text");
  return out;
}
