import { Component, type ErrorInfo, type ReactNode } from "react";
import { describeError, log } from "../lib/log";

export function FatalError({ title, details }: { title: string; details: string }) {
  return (
    <div className="fatal">
      <h2>{title}</h2>
      <pre>{details}</pre>
      <p className="muted">The same message is in the terminal's log. Run with PR_REVIEW_LOG=debug for more.</p>
    </div>
  );
}

/** Shows what went wrong (and logs it) instead of leaving a blank or half-drawn window. */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null };

  static getDerivedStateFromError(e: unknown) {
    return { error: describeError(e) };
  }

  componentDidCatch(e: unknown, info: ErrorInfo) {
    log("error", `rendering failed: ${describeError(e)}\ncomponent stack:${info.componentStack}`);
  }

  render() {
    if (this.state.error !== null) return <FatalError title="Something went wrong" details={this.state.error} />;
    return this.props.children;
  }
}
