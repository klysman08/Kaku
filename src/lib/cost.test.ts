import { describe, expect, it } from "vitest";
import { estimateCost, fillDays } from "./cost";

describe("estimateCost", () => {
  it("sums priced models and ignores unpriced ones", () => {
    const cost = estimateCost(
      [
        { key: "a", requests: 1, inputTokens: 1_000_000, outputTokens: 500_000 },
        { key: "b", requests: 1, inputTokens: 999, outputTokens: 999 },
      ],
      { a: { input: 0.3, output: 2.5 } },
    );
    expect(cost).toBeCloseTo(0.3 + 1.25);
  });
});

describe("fillDays", () => {
  it("fills gaps with empty buckets ending today", () => {
    const today = new Date(2026, 10, 4);
    const rows = fillDays([{ day: "2026-11-03", n: 5 }], 3, (day) => ({ day, n: 0 }), today);
    expect(rows).toEqual([
      { day: "2026-11-02", n: 0 },
      { day: "2026-11-03", n: 5 },
      { day: "2026-11-04", n: 0 },
    ]);
  });
});
