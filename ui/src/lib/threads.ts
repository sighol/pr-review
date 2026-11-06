import type { Side } from "./diff";

export interface FakeComment {
  author: string;
  body: string;
}

export interface FakeThread {
  id: string;
  side: Side;
  line: number;
  resolved: boolean;
  comments: FakeComment[];
}

export const FAKE_THREADS: Record<string, FakeThread[]> = {
  "small-main": [
    {
      id: "t1",
      side: "new",
      line: 12,
      resolved: false,
      comments: [
        { author: "alice", body: "Do we really need `PathBuf` here, or can `traverse_up_to_git_repo` borrow?" },
        { author: "bob", body: "It walks up parents and returns an owned path, so `PathBuf` is needed." },
      ],
    },
    {
      id: "t2",
      side: "new",
      line: 47,
      resolved: true,
      comments: [{ author: "alice", body: "`panic!` is harsh for a CLI. Maybe `bail!` instead?" }, { author: "bob", body: "Fixed in the follow-up." }],
    },
  ],
  "medium-grid": [
    {
      id: "t3",
      side: "new",
      line: 31,
      resolved: false,
      comments: [{ author: "carol", body: "Could this header move into its own `render_header` function?" }],
    },
  ],
};
