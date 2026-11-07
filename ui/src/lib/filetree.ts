export interface FileLeaf {
  kind: "file";
  /** Full path, used as the row key and to select the file. */
  path: string;
  name: string;
}

export interface FolderNode {
  kind: "folder";
  /** Display name, e.g. "src/components" when single-child chains were compacted. */
  name: string;
  /** Full directory path of the last (innermost) segment; used as the row key. */
  path: string;
  children: TreeNode[];
}

export type TreeNode = FolderNode | FileLeaf;

interface RawFolder {
  name: string;
  path: string;
  folders: Map<string, RawFolder>;
  files: { path: string; name: string }[];
}

function rawFolder(name: string, path: string): RawFolder {
  return { name, path, folders: new Map(), files: [] };
}

function buildRaw(paths: readonly string[]): RawFolder {
  const root = rawFolder("", "");
  for (const p of paths) {
    const parts = p.split("/");
    let node = root;
    let acc = "";
    for (let i = 0; i < parts.length - 1; i++) {
      acc = acc ? `${acc}/${parts[i]}` : parts[i];
      let next = node.folders.get(parts[i]);
      if (!next) {
        next = rawFolder(parts[i], acc);
        node.folders.set(parts[i], next);
      }
      node = next;
    }
    node.files.push({ path: p, name: parts[parts.length - 1] });
  }
  return root;
}

const byName = (a: { name: string }, b: { name: string }) => a.name.localeCompare(b.name);

function children(folder: RawFolder): TreeNode[] {
  const subfolders = [...folder.folders.values()].map(collapse).sort(byName);
  const files: FileLeaf[] = folder.files.map((f) => ({ kind: "file" as const, ...f })).sort(byName);
  return [...subfolders, ...files];
}

/** A folder whose only content is a single subfolder (no files of its own) merges into it. */
function collapse(folder: RawFolder): FolderNode {
  const kids = children(folder);
  if (kids.length === 1 && kids[0].kind === "folder") {
    return { kind: "folder", name: `${folder.name}/${kids[0].name}`, path: kids[0].path, children: kids[0].children };
  }
  return { kind: "folder", name: folder.name, path: folder.path, children: kids };
}

/** Builds a nested tree from file paths, sorted folders-then-files alphabetically at each level. */
export function buildFileTree(paths: readonly string[]): TreeNode[] {
  return children(buildRaw(paths));
}
