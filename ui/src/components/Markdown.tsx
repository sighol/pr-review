import type { ThemedToken } from "shiki";
import { useEffect, useState, type ComponentProps } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { tokenizeLang } from "../lib/highlight";

function Highlighted({ code, lang }: { code: string; lang: string | null }) {
  const [tokens, setTokens] = useState<ThemedToken[][] | null>(null);
  useEffect(() => {
    let cancelled = false;
    tokenizeLang(code, lang).then((t) => !cancelled && setTokens(t));
    return () => {
      cancelled = true;
    };
  }, [code, lang]);

  const lines = code.split("\n");
  return (
    <>
      {lines.map((line, i) => (
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

export function Markdown({ text, fileLang }: { text: string; fileLang?: string | null }) {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
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
                  <div className="md-code">
                    <Highlighted code={code} lang={fileLang ?? null} />
                  </div>
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
