import { useMemo, useState, type ReactNode } from "react";
import { buildFileTree, type TreeNode } from "../lib/filetree";

export interface FileStatus {
  approved: boolean;
  /** Threads resolved out of the total; N/N means nothing left to act on. */
  resolved: number;
  total: number;
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

  return <nav className="file-tree">{tree.map((n) => renderNode(n, 0))}</nav>;
}
