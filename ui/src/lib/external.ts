import { describeError, log } from "./log";

/** Links that leave the app: web pages and mail. Anything else (relative paths, `#anchors`) stays inside. */
export const isExternalUrl = (href: string) => /^(https?:|mailto:)/i.test(href);

/** Opens a URL in the default browser (Tauri) or a new tab (browser). */
export function openExternal(url: string) {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    import("@tauri-apps/plugin-opener")
      .then(({ openUrl }) => openUrl(url))
      .catch((e) => log("error", `could not open ${url}: ${describeError(e)}`));
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
