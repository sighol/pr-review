import { describe, expect, test } from "vitest";
import { buildFileTree, flattenFiles, type FolderNode } from "./filetree";

describe("buildFileTree", () => {
  test("flat files sort alphabetically with no folders", () => {
    const tree = buildFileTree(["b.rs", "a.rs"]);
    expect(tree).toEqual([
      { kind: "file", path: "a.rs", name: "a.rs" },
      { kind: "file", path: "b.rs", name: "b.rs" },
    ]);
  });

  test("folders sort before files at the same level", () => {
    const tree = buildFileTree(["z.rs", "a/x.rs"]);
    expect(tree.map((n) => n.kind)).toEqual(["folder", "file"]);
  });

  test("a folder with only one subfolder and no files of its own compacts its name", () => {
    const tree = buildFileTree(["src/components/Foo.tsx"]);
    expect(tree).toHaveLength(1);
    const node = tree[0] as FolderNode;
    expect(node.kind).toBe("folder");
    expect(node.name).toBe("src/components");
    expect(node.path).toBe("src/components");
    expect(node.children).toEqual([{ kind: "file", path: "src/components/Foo.tsx", name: "Foo.tsx" }]);
  });

  test("a folder with a file of its own does not compact even with one subfolder", () => {
    const tree = buildFileTree(["src/main.rs", "src/components/Foo.tsx"]);
    expect(tree).toHaveLength(1);
    const src = tree[0] as FolderNode;
    expect(src.name).toBe("src");
    expect(src.children.map((n) => n.kind)).toEqual(["folder", "file"]);
  });

  test("a folder with multiple subfolders does not compact", () => {
    const tree = buildFileTree(["src/a/x.rs", "src/b/y.rs"]);
    expect(tree).toHaveLength(1);
    const src = tree[0] as FolderNode;
    expect(src.name).toBe("src");
    expect(src.children.map((n) => n.name)).toEqual(["a", "b"]);
  });

  test("chains of single-child folders compact across multiple levels", () => {
    const tree = buildFileTree(["a/b/c/d.rs"]);
    expect(tree).toEqual([
      {
        kind: "folder",
        name: "a/b/c",
        path: "a/b/c",
        children: [{ kind: "file", path: "a/b/c/d.rs", name: "d.rs" }],
      },
    ]);
  });
});

describe("flattenFiles", () => {
  test("walks folders before files, depth-first", () => {
    const tree = buildFileTree(["z.rs", "a/x.rs", "a/y.rs"]);
    expect(flattenFiles(tree)).toEqual(["a/x.rs", "a/y.rs", "z.rs"]);
  });
});
