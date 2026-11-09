export function ProgressPopup({ lines }: { lines: string[] }) {
  return (
    <div className="progress-popup-backdrop">
      <div className="progress-popup">
        {lines.map((line, i) => (
          <div className="progress-line" key={i}>
            {line}
          </div>
        ))}
      </div>
    </div>
  );
}
