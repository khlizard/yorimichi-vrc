import { describe, expect, it } from "vitest";
import { fmtDur, fmtSize, nf, pad } from "./format";

describe("nf", () => {
  it("groups thousands the ja-JP way", () => {
    expect(nf(0)).toBe("0");
    expect(nf(1234567)).toBe("1,234,567");
  });
});

describe("pad", () => {
  it("pads to two digits", () => {
    expect(pad(0)).toBe("00");
    expect(pad(7)).toBe("07");
    expect(pad(12)).toBe("12");
  });
});

describe("fmtDur", () => {
  it("shows minutes only under an hour", () => {
    expect(fmtDur(0)).toBe("0m");
    expect(fmtDur(45 * 60 + 59)).toBe("45m");
  });
  it("shows hours with zero-padded minutes", () => {
    expect(fmtDur(3600)).toBe("1h 00m");
    expect(fmtDur(3 * 3600 + 12 * 60)).toBe("3h 12m");
  });
  it("clamps negatives and drops fractions", () => {
    expect(fmtDur(-5)).toBe("0m");
    expect(fmtDur(61.9)).toBe("1m");
  });
});

describe("fmtSize", () => {
  const MB = 1024 * 1024;
  it("mb: whole MB without separators", () => {
    expect(fmtSize(0, "mb")).toBe("0 MB");
    expect(fmtSize(5 * MB, "mb")).toBe("5 MB");
    expect(fmtSize(1500 * MB, "mb")).toBe("1500 MB");
  });
  it("gb: whole MB, then one-decimal GB from 1 GB", () => {
    expect(fmtSize(1, "gb")).toBe("1 MB");
    expect(fmtSize(900 * MB, "gb")).toBe("900 MB");
    expect(fmtSize(1024 * MB - 1, "gb")).toBe("1,024 MB");
    expect(fmtSize(1024 * MB, "gb")).toBe("1.0 GB");
    expect(fmtSize(1536 * MB, "gb")).toBe("1.5 GB");
  });
  it("kb: whole KB, then one-decimal MB from 1 MB", () => {
    expect(fmtSize(10, "kb")).toBe("1 KB");
    expect(fmtSize(512 * 1024, "kb")).toBe("512 KB");
    expect(fmtSize(MB, "kb")).toBe("1 MB");
    expect(fmtSize(Math.round(2.54 * MB), "kb")).toBe("2.5 MB");
    expect(fmtSize(2048 * MB, "kb")).toBe("2,048 MB");
  });
});
