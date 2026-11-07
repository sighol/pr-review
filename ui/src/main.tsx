import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { describeViewport, installErrorLogging, isViewportBroken, log } from "./lib/log";
import "./styles.css";

const RETRIES_KEY = "pr-review-viewport-retries";
const MAX_RETRIES = 5;

const retries = () => {
  try {
    return Number(sessionStorage.getItem(RETRIES_KEY) ?? "0");
  } catch {
    return 0;
  }
};
const setRetries = (n: number) => {
  try {
    if (n > 0) sessionStorage.setItem(RETRIES_KEY, String(n));
    else sessionStorage.removeItem(RETRIES_KEY);
  } catch {
    // Without sessionStorage we cannot count reloads, so we just stop retrying below.
  }
};

function start() {
  installErrorLogging();
  log("info", `frontend starting at ${location.href}`);
  log("info", describeViewport());

  const tries = retries();
  if (isViewportBroken(window.devicePixelRatio, window.innerWidth, window.innerHeight)) {
    if (tries < MAX_RETRIES) {
      setRetries(tries + 1);
      log("warn", `the webview reports a broken viewport; reloading (attempt ${tries + 1} of ${MAX_RETRIES})`);
      setTimeout(() => location.reload(), 300);
      return;
    }
    log("error", `the viewport is still broken after ${MAX_RETRIES} reloads; showing the app anyway`);
  } else if (tries > 0) {
    log("info", `the viewport is fine after ${tries} reload${tries === 1 ? "" : "s"}`);
    setRetries(0);
  }

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ErrorBoundary>
        <App />
      </ErrorBoundary>
    </StrictMode>,
  );
}

start();
