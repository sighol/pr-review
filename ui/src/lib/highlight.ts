import { bundledLanguages, createHighlighter, type Highlighter, type ThemedToken } from "shiki";

export const THEME = "github-dark-default";

// Extensions that are not already a Shiki language name or alias (those are used as they are).
const LANG_BY_EXT: Record<string, string> = {
  gradle: "groovy",
  mjs: "javascript",
  cjs: "javascript",
};

const LANG_BY_FILENAME: Record<string, string> = {
  Dockerfile: "dockerfile",
  Makefile: "makefile",
  justfile: "just",
};

const isBundled = (lang: string) => Object.hasOwn(bundledLanguages, lang);

let highlighterPromise: Promise<Highlighter> | null = null;

function getHighlighter() {
  highlighterPromise ??= createHighlighter({ themes: [THEME], langs: [] });
  return highlighterPromise;
}

/** The Shiki language for a file path (by file name, then extension), or null if none fits. */
export function langFor(path: string): string | null {
  const name = path.split("/").pop() ?? path;
  if (LANG_BY_FILENAME[name]) return LANG_BY_FILENAME[name];
  if (!name.includes(".")) return null;
  const ext = name.split(".").pop()!.toLowerCase();
  const lang = LANG_BY_EXT[ext] ?? ext;
  return isBundled(lang) ? lang : null;
}

export async function tokenize(text: string, path: string): Promise<ThemedToken[][] | null> {
  return tokenizeLang(text, langFor(path));
}

export async function tokenizeLang(text: string, lang: string | null | undefined): Promise<ThemedToken[][] | null> {
  if (!lang) return null;
  lang = LANG_BY_EXT[lang] ?? lang;
  if (!isBundled(lang)) return null;
  const hl = await getHighlighter();
  if (!hl.getLoadedLanguages().includes(lang)) await hl.loadLanguage(lang as never);
  return hl.codeToTokensBase(text, { lang: lang as never, theme: THEME });
}
