import { useEffect, useMemo, useState, type ReactNode } from "react";
import { openInEditor } from "../lib/backend";
import { copyText } from "../lib/clipboard";
import { buildFileTree, flattenFiles, type TreeNode } from "../lib/filetree";
import { describeError, log } from "../lib/log";

export interface FileStatus {
  approved: boolean;
  /** Threads resolved out of the total; N/N means nothing left to act on. */
  resolved: number;
  total: number;
}

interface Menu {
  path: string;
  /** Set for a folder: every file below it. */
  folderFiles?: string[];
  x: number;
  y: number;
}

const EDITORS = ["zed", "code", "idea"] as const;

export function FileTree({
  paths,
  currentFile,
  onSelect,
  statusFor,
  renamedFrom,
  onSetApproved,
  width,
}: {
  paths: readonly string[];
  currentFile: string;
  onSelect: (path: string) => void;
  statusFor: (path: string) => FileStatus;
  /** Renamed files: path -> the path at the PR base. They get a bold R. */
  renamedFrom: ReadonlyMap<string, string>;
  /** Approves or unapproves all the given files. */
  onSetApproved: (paths: string[], approved: boolean) => void;
  width: number;
}) {
  const tree = useMemo(() => buildFileTree(paths), [paths]);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const toggle = (path: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const [menu, setMenu] = useState<Menu | null>(null);
  useEffect(() => {
    if (!menu) return;
    const close = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [menu]);

  const renderNode = (node: TreeNode, depth: number): ReactNode => {
    const indent = { paddingLeft: depth * 14 + 8 };
    if (node.kind === "file") {
      const s = statusFor(node.path);
      return (
        <button
          key={node.path}
          className={`ft-file${node.path === currentFile ? " current" : ""}`}
          style={indent}
          onClick={() => onSelect(node.path)}
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu({ path: node.path, x: e.clientX, y: e.clientY });
          }}
        >
          <span className={`status-dot ${s.approved ? "approved" : "unapproved"}`} />
          {renamedFrom.has(node.path) && (
            <span className="ft-renamed" title={`Renamed from ${renamedFrom.get(node.path)}`}>
              R
            </span>
          )}
          <span className="ft-name">{node.name}</span>
          {s.total > 0 && (
            <span className={`ft-count${s.resolved < s.total ? " attention" : ""}`}>
              {s.resolved}/{s.total}
            </span>
          )}
        </button>
      );
    }
    const isCollapsed = collapsed.has(node.path);
    return (
      <div key={node.path}>
        <button
          className="ft-folder"
          style={indent}
          onClick={() => toggle(node.path)}
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu({ path: node.path, folderFiles: flattenFiles(node.children), x: e.clientX, y: e.clientY });
          }}
        >
          <span className={`ft-chevron${isCollapsed ? "" : " open"}`} />
          {node.name}
        </button>
        {!isCollapsed && node.children.map((c) => renderNode(c, depth + 1))}
      </div>
    );
  };

  return (
    <nav className="file-tree" style={{ width }}>
      {tree.map((n) => renderNode(n, 0))}
      {menu && (
        <>
          <div className="ft-menu-backdrop" onClick={() => setMenu(null)} onContextMenu={(e) => e.preventDefault()} />
          <div className="ft-menu" style={{ left: menu.x, top: menu.y }}>
            {menu.folderFiles ? (
              <>
                <button
                  onClick={() => {
                    onSetApproved(menu.folderFiles!, true);
                    setMenu(null);
                  }}
                >
                  Approve all
                </button>
                <button
                  onClick={() => {
                    onSetApproved(menu.folderFiles!, false);
                    setMenu(null);
                  }}
                >
                  Unapprove all
                </button>
              </>
            ) : (
              <>
                <button
                  onClick={() => {
                    copyText(menu.path);
                    setMenu(null);
                  }}
                >
                  Copy path
                </button>
                {EDITORS.map((program) => (
                  <button
                    key={program}
                    onClick={() => {
                      openInEditor(menu.path, program).catch((e) =>
                        log("warn", `could not open ${menu.path} in ${program}: ${describeError(e)}`),
                      );
                      setMenu(null);
                    }}
                  >
                    Open in {program}
                  </button>
                ))}
              </>
            )}
          </div>
        </>
      )}
    </nav>
  );
}
