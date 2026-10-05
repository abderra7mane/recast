import { describe, expect, it } from "vitest";

import { createLatestRunner, previewSize } from "@/beautify/latest";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
}

describe("createLatestRunner", () => {
  it("runs the first value and then only the newest", async () => {
    const runs: number[] = [];
    const gates: ReturnType<typeof deferred>[] = [];
    const runner = createLatestRunner(async (value: number) => {
      runs.push(value);
      const gate = deferred();
      gates.push(gate);
      await gate.promise;
    });
    runner.push(1);
    runner.push(2);
    runner.push(3);
    expect(runs).toEqual([1]);
    gates[0].resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(runs).toEqual([1, 3]);
    gates[1].resolve();
    await Promise.resolve();
    runner.push(4);
    await Promise.resolve();
    expect(runs).toEqual([1, 3, 4]);
  });

  it("keeps going after a failed run", async () => {
    const runs: number[] = [];
    const runner = createLatestRunner(async (value: number) => {
      runs.push(value);
      if (value === 1) throw new Error("render failed");
    });
    runner.push(1);
    await new Promise((r) => setTimeout(r, 0));
    runner.push(2);
    await new Promise((r) => setTimeout(r, 0));
    expect(runs).toEqual([1, 2]);
  });
});

describe("previewSize", () => {
  it("scales CSS pixels to device pixels", () => {
    expect(previewSize(800, 500.4, 2)).toEqual({ width: 1600, height: 1001 });
    expect(previewSize(0, 3, 1)).toEqual({ width: 16, height: 16 });
  });
});
