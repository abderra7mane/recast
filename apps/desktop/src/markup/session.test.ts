import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn(async () => undefined));
const markupFinish = vi.hoisted(() =>
  vi.fn(async () => ({ status: "ok", data: null })),
);
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@/bindings", () => ({ commands: { markupFinish } }));
vi.mock("@/markup/render", () => ({
  createRenderer: vi.fn(),
  renderOutput: () => new Uint8ClampedArray(4 * 4 * 4),
}));

const { finish, oneAtATime } = await import("@/markup/session");

const renderer = {
  scale: 2,
  image: { width: 4, height: 4 },
} as Parameters<typeof finish>[0];

const doc = (edited: boolean) =>
  ({
    shapes: edited
      ? [{ id: "s1", kind: "blur", rect: { x: 0, y: 0, width: 2, height: 2 } }]
      : [],
    crop: null,
    beautify: false,
    background: {},
  }) as unknown as Parameters<typeof finish>[1];

describe("finishing", () => {
  beforeEach(() => {
    invoke.mockClear();
    markupFinish.mockClear();
  });

  it("names the staged image in the finish, a new token each time", async () => {
    await finish(renderer, doc(true), { kind: "copy" });
    await finish(renderer, doc(true), { kind: "done" });
    const tokens = invoke.mock.calls.map(
      (call) => (call as unknown[])[2] as { headers: Record<string, string> },
    );
    expect(tokens.map((t) => t.headers["x-recast-width"])).toEqual(["4", "4"]);
    const sent = tokens.map((t) => Number(t.headers["x-recast-token"]));
    expect(sent[0]).not.toBe(sent[1]);
    expect(
      markupFinish.mock.calls.map((call) => (call as unknown[])[1]),
    ).toEqual(sent);
  });

  it("sends an unedited screenshot as taken, without a token", async () => {
    await finish(renderer, doc(false), { kind: "copy" });
    expect(invoke).not.toHaveBeenCalled();
    expect(markupFinish).toHaveBeenCalledWith({ kind: "copy" }, null, null);
  });
});

describe("oneAtATime", () => {
  it("ignores calls while one runs", async () => {
    const runs: string[] = [];
    let release!: () => void;
    const run = oneAtATime(async (name: string) => {
      runs.push(name);
      await new Promise<void>((r) => (release = r));
    });
    const first = run("copy");
    void run("done");
    void run("copy");
    expect(runs).toEqual(["copy"]);
    release();
    await first;
    void run("done");
    expect(runs).toEqual(["copy", "done"]);
    release();
  });

  it("runs again after a failure", async () => {
    let calls = 0;
    const run = oneAtATime(async () => {
      calls++;
      throw new Error("failed");
    });
    await expect(run()).rejects.toThrow();
    await expect(run()).rejects.toThrow();
    expect(calls).toBe(2);
  });
});
