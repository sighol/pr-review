import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { describeViewport, installErrorLogging, log } from "./lib/log";
import "./styles.css";

installErrorLogging();
log("info", `frontend starting at ${location.href}`);
log("info", describeViewport());

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
