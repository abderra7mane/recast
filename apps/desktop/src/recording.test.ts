import { describe, expect, it } from "vitest";

import { formatElapsed } from "@/recording";

describe("formatElapsed", () => {
  it("formats minutes and seconds", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(61_900)).toBe("1:01");
    expect(formatElapsed(null)).toBe("0:00");
  });
});
