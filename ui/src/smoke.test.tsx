import { beforeAll, describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { renderToString } from "react-dom/server";

// Render-time smoke test: real components, real fixtures, no browser.
beforeAll(() => {
  Object.assign(globalThis, {
    location: { pathname: "/", search: "" },
    history: { pushState() {} },
    fetch: async (url: string) => {
      const body = readFileSync(`public${url}`, "utf8");
      return { text: async () => body, json: async () => JSON.parse(body) };
    },
  });
});

async function setup() {
  const { loadAll } = await import("./App");
  const { StoreProvider } = await import("./store");
  return { loaded: await loadAll(), StoreProvider };
}

describe("renders without throwing", () => {
  test("summary, diff and review tabs", async () => {
    const { loaded, StoreProvider } = await setup();
    const { SummaryTab } = await import("./tabs/SummaryTab");
    const { DiffTab } = await import("./tabs/DiffTab");
    const { ReviewTab } = await import("./tabs/ReviewTab");
    const html = renderToString(
      <StoreProvider files={loaded.files} commits={loaded.commits} threads={loaded.threads} pr={loaded.pr}>
        <SummaryTab />
        <DiffTab />
        <ReviewTab />
      </StoreProvider>,
    );
    expect(html).toContain("Files");
    expect(html).toContain("Discussions on code");
  });

  test("diff view for several ranges of every file", async () => {
    const { loaded, StoreProvider } = await setup();
    const { DiffView } = await import("./components/DiffView");
    const { loadRangeData } = await import("./lib/data");
    for (const file of loaded.files) {
      for (const range of [
        { from: 0, to: 18 },
        { from: 5, to: 18 },
        { from: 0, to: 9 },
        { from: 3, to: 12 },
      ]) {
        const data = await loadRangeData(file, range);
        const html = renderToString(
          <StoreProvider files={loaded.files} commits={loaded.commits} threads={loaded.threads} pr={loaded.pr}>
            <DiffView file={file} data={data} range={range} wholeFile={false} />
          </StoreProvider>,
        );
        expect(html.length).toBeGreaterThan(0);
      }
    }
  });
});
