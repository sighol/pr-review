type Level = "error" | "warn" | "info" | "debug";

const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Writes to the app's log (the terminal's stderr) in Tauri. In a browser only warnings and
 * errors go to the console, so tests and `bun run dev` stay quiet.
 */
export function log(level: Level, message: string) {
  if (!inTauri()) {
    if (level === "error" || level === "warn") console[level](`[pr-review] ${message}`);
    return;
  }
  import("@tauri-apps/api/core")
    .then(({ invoke }) => invoke("frontend_log", { level, message }))
    .catch(() => {});
}

export function describeError(e: unknown): string {
  if (e instanceof Error) return e.stack ? `${e.message}\n${e.stack}` : e.message;
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

const formatArg = (a: unknown) => (typeof a === "string" ? a : describeError(a));

/** In Tauri, sends uncaught errors, unhandled rejections, failed asset loads and console errors to the log. */
export function installErrorLogging() {
  if (!inTauri()) return;

  window.addEventListener("error", (e) => {
    if (e.target && e.target !== window && e.target instanceof HTMLElement) {
      const src = (e.target as HTMLScriptElement).src || (e.target as HTMLLinkElement).href;
      log("error", `failed to load <${e.target.tagName.toLowerCase()}> ${src}`);
      return;
    }
    const where = e.filename ? ` at ${e.filename}:${e.lineno}:${e.colno}` : "";
    log("error", `uncaught error: ${e.message}${where}${e.error?.stack ? `\n${e.error.stack}` : ""}`);
  }, true);

  window.addEventListener("unhandledrejection", (e) => {
    log("error", `unhandled promise rejection: ${describeError(e.reason)}`);
  });

  for (const level of ["error", "warn"] as const) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      original(...args);
      log(level, `console.${level}: ${args.map(formatArg).join(" ")}`);
    };
  }
}

/** One line about the webview, to diagnose rendering problems (size, zoom, fonts). */
export function describeViewport(): string {
  const root = getComputedStyle(document.documentElement);
  const body = getComputedStyle(document.body);
  return (
    `window ${window.innerWidth}x${window.innerHeight} (outer ${window.outerWidth}x${window.outerHeight}), ` +
    `screen ${screen.width}x${screen.height}, devicePixelRatio ${window.devicePixelRatio}, ` +
    `visualViewport scale ${window.visualViewport?.scale}, ` +
    `root font-size ${root.fontSize}, body font ${body.fontSize} ${body.fontFamily}`
  );
}

/**
 * The webview sometimes starts with a negative pixel ratio and a negative window size
 * (seen with WebKitGTK on Wayland); nothing can be laid out then.
 */
export function isViewportBroken(pixelRatio: number, width: number, height: number): boolean {
  return !(pixelRatio > 0 && width > 0 && height > 0);
}
