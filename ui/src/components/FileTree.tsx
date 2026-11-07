import { useEffect, useMemo, useState, type ReactNode } from "react";
import { copyText } from "../lib/clipboard";
import { buildFileTree, type TreeNode } from "../lib/filetree";

export interface FileStatus {
  approved: boolean;
  /** Threads resolved out of the total; N/N means nothing left to act on. */
  resolved: number;
  total: number;
}

interface Menu {
  path: string;
  x: number;
  y: number;
}

export function FileTree({
  paths,
  currentFile,
  onSelect,
  statusFor,
}: {
  paths: readonly string[];
  currentFile: string;
  onSelect: (path: string) => void;
  statusFor: (path: string) => FileStatus;
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
          <span className={`ft-dot ${s.approved ? "approved" : "unapproved"}`} />
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
        <button className="ft-folder" style={indent} onClick={() => toggle(node.path)}>
          <span className={`ft-chevron${isCollapsed ? "" : " open"}`} />
          {node.name}
        </button>
        {!isCollapsed && node.children.map((c) => renderNode(c, depth + 1))}
      </div>
    );
  };

  return (
    <nav className="file-tree">
      {tree.map((n) => renderNode(n, 0))}
      {menu && (
        <>
          <div className="ft-menu-backdrop" onClick={() => setMenu(null)} onContextMenu={(e) => e.preventDefault()} />
          <div className="ft-menu" style={{ left: menu.x, top: menu.y }}>
            <button
              onClick={() => {
                copyText(menu.path);
                setMenu(null);
              }}
            >
              Copy path
            </button>
          </div>
        </>
      )}
    </nav>
  );
}
