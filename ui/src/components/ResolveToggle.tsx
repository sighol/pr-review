export function ResolveToggle({ on, onChange }: { on: boolean; onChange: (on: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      className={`resolve-toggle${on ? " on" : ""}`}
      title={
        on
          ? "This thread will be resolved, up to and including this comment, when the review is sent"
          : "Resolve this thread, up to and including this comment, when the review is sent"
      }
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => onChange(!on)}
    >
      <span className="resolve-track">
        <span className="resolve-knob" />
      </span>
      resolve
    </button>
  );
}
