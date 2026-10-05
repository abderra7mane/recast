import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createSequence, createSettingsSync } from "@/editor/sync";

describe("settings sync", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("sends the last of rapid changes once per interval", () => {
    const send = vi.fn();
    const sync = createSettingsSync<number>(send, 100);
    sync.schedule(1);
    sync.schedule(2);
    vi.advanceTimersByTime(60);
    sync.schedule(3);
    expect(send).not.toHaveBeenCalled();
    vi.advanceTimersByTime(40);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenLastCalledWith(3);

    sync.schedule(4);
    vi.advanceTimersByTime(99);
    expect(send).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(send).toHaveBeenLastCalledWith(4);
    vi.advanceTimersByTime(1000);
    expect(send).toHaveBeenCalledTimes(2);
  });

  it("keeps sending while changes continue", () => {
    const send = vi.fn();
    const sync = createSettingsSync<number>(send, 100);
    for (let i = 0; i < 50; i++) {
      sync.schedule(i);
      vi.advanceTimersByTime(10);
    }
    vi.advanceTimersByTime(100);
    expect(send.mock.calls.length).toBeGreaterThanOrEqual(5);
    expect(send).toHaveBeenLastCalledWith(49);
  });

  it("commits right away and drops the scheduled value", () => {
    const send = vi.fn();
    const sync = createSettingsSync<string>(send, 100);
    sync.schedule("draft");
    sync.commit("final");
    expect(send).toHaveBeenCalledExactlyOnceWith("final");
    vi.advanceTimersByTime(500);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("flushes a scheduled value", () => {
    const send = vi.fn();
    const sync = createSettingsSync<string>(send, 100);
    sync.flush();
    expect(send).not.toHaveBeenCalled();
    sync.schedule("a");
    sync.flush();
    expect(send).toHaveBeenCalledExactlyOnceWith("a");
    vi.advanceTimersByTime(500);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("cancel drops a scheduled value", () => {
    const send = vi.fn();
    const sync = createSettingsSync<string>(send, 100);
    sync.schedule("a");
    sync.cancel();
    vi.advanceTimersByTime(500);
    expect(send).not.toHaveBeenCalled();
  });
});

describe("sequence", () => {
  it("accepts only results newer than the newest accepted", () => {
    const sequence = createSequence();
    const [a, b, c] = [sequence.next(), sequence.next(), sequence.next()];
    expect([a, b, c]).toEqual([1, 2, 3]);
    expect(sequence.accept(b)).toBe(true);
    expect(sequence.accept(a)).toBe(false);
    expect(sequence.accept(b)).toBe(false);
    expect(sequence.accept(c)).toBe(true);
  });
});
