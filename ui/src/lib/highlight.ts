import { bundledLanguages, createHighlighter, type Highlighter, type ThemedToken } from "shiki";

export const THEME = "github-dark-default";

const LANG_BY_EXT: Record<string, string> = {
  rs: "rust",
  toml: "toml",
  ts: "typescript",
  tsx: "tsx",
  js: "javascript",
  py: "python",
  md: "markdown",
  sql: "sql",
  json: "json",
};

let highlighterPromise: Promise<Highlighter> | null = null;

function getHighlighter() {
  highlighterPromise ??= createHighlighter({ themes: [THEME], langs: [] });
  return highlighterPromise;
}

export function langFor(path: string): string | null {
  const ext = path.split(".").pop() ?? "";
  return LANG_BY_EXT[ext] ?? null;
}

export async function tokenize(text: string, path: string): Promise<ThemedToken[][] | null> {
  return tokenizeLang(text, langFor(path));
}

export async function tokenizeLang(text: string, lang: string | null | undefined): Promise<ThemedToken[][] | null> {
  if (!lang) return null;
  lang = LANG_BY_EXT[lang] ?? lang;
  if (!(lang in bundledLanguages)) return null;
  const hl = await getHighlighter();
  if (!hl.getLoadedLanguages().includes(lang)) await hl.loadLanguage(lang as never);
  return hl.codeToTokensBase(text, { lang: lang as never, theme: THEME });
}
