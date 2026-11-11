import type { ThemedToken } from "shiki";
import { useEffect, useMemo, useState, type ComponentProps } from "react";
import ReactMarkdown from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { segmentsFor, wordRanges, type Range } from "../lib/diff";
import { tokenizeLang } from "../lib/highlight";
import { ExternalLink } from "./ExternalLink";

function useTokens(code: string, lang: string | null) {
  const [tokens, setTokens] = useState<ThemedToken[][] | null>(null);
  useEffect(() => {
    let cancelled = false;
    tokenizeLang(code, lang).then((t) => !cancelled && setTokens(t));
    return () => {
      cancelled = true;
    };
  }, [code, lang]);
  return tokens;
}

function Highlighted({ code, lang }: { code: string; lang: string | null }) {
  const tokens = useTokens(code, lang);
  return (
    <>
      {code.split("\n").map((line, i) => (
        <div className="md-code-line" key={i}>
          {tokens?.[i]
            ? tokens[i].map((t, j) => (
                <span key={j} style={{ color: t.color }}>
                  {t.content}
                </span>
              ))
            : line}
          {line === "" && "​"}
        </div>
      ))}
    </>
  );
}

function DiffLine({
  sign,
  text,
  tokens,
  ranges,
}: {
  sign: "-" | "+";
  text: string;
  tokens: ThemedToken[] | undefined;
  ranges: Range[] | undefined;
}) {
  return (
    <div className={`md-diff-line ${sign === "-" ? "del" : "add"}`}>
      <span className="md-diff-sign">{sign}</span>
      <span className="md-diff-text">
        {segmentsFor(tokens, text, ranges).map((s, i) =>
          s.changed ? (
            <mark key={i} style={{ color: s.color }}>
              {s.text}
            </mark>
          ) : (
            <span key={i} style={{ color: s.color }}>
              {s.text}
            </span>
          ),
        )}
        {text === "" && "​"}
      </span>
    </div>
  );
}

/** GitHub-style suggested change: replaced lines in red, suggested lines in green. */
function SuggestionDiff({ base, code, lang }: { base: string[]; code: string; lang: string | null }) {
  const suggested = code.split("\n");
  const baseTokens = useTokens(base.join("\n"), lang);
  const newTokens = useTokens(code, lang);
  const ranges = useMemo(
    () =>
      suggested.map((line, i) => (i < base.length ? wordRanges(base[i], line) : null)),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [base, code],
  );

  return (
    <div className="md-code md-diff">
      {base.map((line, i) => (
        <DiffLine key={`o${i}`} sign="-" text={line} tokens={baseTokens?.[i]} ranges={ranges[i]?.old} />
      ))}
      {suggested.map((line, i) => (
        <DiffLine key={`n${i}`} sign="+" text={line} tokens={newTokens?.[i]} ranges={ranges[i]?.new} />
      ))}
    </div>
  );
}

interface Props {
  text: string;
  fileLang?: string | null;
  /** The current lines a suggestion block in this text would replace. */
  replaces?: string[] | null;
}

export function Markdown({ text, fileLang, replaces }: Props) {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        // Raw HTML (tables, comments, <sub>, ...) is common in GitHub comments; rehype-raw parses it
        // and rehype-sanitize's default (GitHub-like) schema strips anything unsafe, e.g. <script>
        // or javascript: URLs, same as GitHub's own renderer would.
        rehypePlugins={[rehypeRaw, rehypeSanitize]}
        components={{
          a: ({ href, children }) => <ExternalLink href={href}>{children}</ExternalLink>,
          pre: ({ children }) => <>{children}</>,
          code: ({ className, children }: ComponentProps<"code">) => {
            const raw = String(children ?? "");
            const lang = /language-([\w-]+)/.exec(className ?? "")?.[1] ?? null;
            const isBlock = lang !== null || raw.includes("\n");
            if (!isBlock) return <code className="md-inline">{children}</code>;
            const code = raw.replace(/\n$/, "");
            if (lang === "suggestion") {
              return (
                <div className="md-suggestion">
                  <div className="md-suggestion-head">Suggested change</div>
                  {replaces && replaces.length > 0 ? (
                    <SuggestionDiff base={replaces} code={code} lang={fileLang ?? null} />
                  ) : (
                    <div className="md-code add-only">
                      <Highlighted code={code} lang={fileLang ?? null} />
                    </div>
                  )}
                </div>
              );
            }
            return (
              <div className="md-code block">
                <Highlighted code={code} lang={lang} />
              </div>
            );
          },
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
