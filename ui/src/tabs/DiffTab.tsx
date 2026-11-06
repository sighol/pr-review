import { useState } from "react";
import { DiffView } from "../components/DiffView";
import { useStore } from "../store";

export function DiffTab() {
  const { files, fileByName, currentFile, setCurrentFile } = useStore();
  const [wholeFile, setWholeFile] = useState(false);
  const file = fileByName.get(currentFile);

  return (
    <>
      <div className="toolbar">
        <select value={currentFile} onChange={(e) => setCurrentFile(e.target.value)}>
          {files.map((f) => (
            <option key={f.name} value={f.name}>
              {f.label}
            </option>
          ))}
        </select>
        <span className="path">{file?.path}</span>
        <button onClick={() => setWholeFile((w) => !w)}>{wholeFile ? "Show changed hunks" : "Show whole file"}</button>
        <span className="hint">Drag a line number on a changed line to comment</span>
      </div>
      {file && <DiffView key={file.name} file={file} wholeFile={wholeFile} />}
    </>
  );
}
