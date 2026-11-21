import { describe, expect, it } from "vitest";
import { dayLabel, logicalDay } from "./photoUtil";

describe("logicalDay", () => {
  it("keeps the date at or after the start hour", () => {
    expect(logicalDay("2026-11-19 05:00:00", 5)).toBe("2026-11-19");
    expect(logicalDay("2026-11-19 23:59:59", 5)).toBe("2026-11-19");
  });

  it("counts times before the start hour as the previous day", () => {
    expect(logicalDay("2026-11-19 04:59:59", 5)).toBe("2026-11-18");
    expect(logicalDay("2026-11-19 00:00:00", 5)).toBe("2026-11-18");
  });

  it("never rolls back when the start hour is 0", () => {
    expect(logicalDay("2026-11-19 00:00:00", 0)).toBe("2026-11-19");
  });

  it("rolls back across month and year boundaries", () => {
    expect(logicalDay("2026-03-01 01:00:00", 5)).toBe("2026-02-28");
    expect(logicalDay("2024-03-01 01:00:00", 5)).toBe("2024-02-29");
    expect(logicalDay("2026-01-01 02:30:00", 5)).toBe("2025-12-31");
  });
});

describe("dayLabel", () => {
  it("formats the date with a weekday", () => {
    expect(dayLabel("2026-11-19")).toBe("2026年11月19日 (木)");
    expect(dayLabel("2026-01-04")).toBe("2026年1月4日 (日)");
  });
});
