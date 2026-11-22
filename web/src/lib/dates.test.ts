import { describe, expect, it } from "vitest";
import { addDays, addMonths, diffDays, monthGrid, parseTyped, startOfWeek, weekdayNames } from "./dates";

// 2026-11-22 is a Sunday.
const today = "2026-11-22";

describe("calendar math", () => {
  it("adds days and months, clamping to the month's end", () => {
    expect(addDays("2026-11-30", 1)).toBe("2026-12-01");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
    expect(addMonths("2026-01-31", 1)).toBe("2026-02-28");
    expect(addMonths("2024-01-31", 1)).toBe("2024-02-29");
    expect(addMonths("2026-11-15", -12)).toBe("2025-11-15");
  });
  it("counts days across a DST change", () => {
    expect(diffDays("2026-03-01", "2026-04-01")).toBe(31);
    expect(diffDays("2026-11-30", "2026-11-22")).toBe(-8);
  });
  it("starts weeks on Sunday or Monday", () => {
    expect(startOfWeek("2026-11-25", 0)).toBe("2026-11-22");
    expect(startOfWeek("2026-11-25", 1)).toBe("2026-11-23");
    expect(startOfWeek("2026-11-22", 1)).toBe("2026-11-16");
  });
  it("lays out six weeks around the month", () => {
    const g = monthGrid("2026-11-10", 0);
    expect(g).toHaveLength(42);
    expect(g[0]).toMatchObject({ iso: "2026-11-01", inMonth: true, weekend: true });
    expect(g[29]).toMatchObject({ iso: "2026-11-30", inMonth: true });
    expect(g[30]).toMatchObject({ iso: "2026-12-01", inMonth: false });
    const m = monthGrid("2026-11-10", 1);
    expect(m[0]!.iso).toBe("2026-10-26");
  });
  it("names weekdays in grid order", () => {
    expect(weekdayNames("en", 0)).toEqual(["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]);
    expect(weekdayNames("id", 1)[0]).toBe("Sen");
  });
});

describe("typed dates", () => {
  const cases: [string, "en" | "id", string | null][] = [
    ["2026-12-24", "en", "2026-12-24"],
    ["2026-2-30", "en", null],
    ["12/24", "en", "2026-12-24"],
    ["24/12", "id", "2026-12-24"],
    ["24/12/27", "id", "2027-12-24"],
    ["24/12", "en", null],
    ["nov 30", "en", "2026-11-30"],
    ["30 November 2027", "en", "2027-11-30"],
    ["Dec 1st", "en", "2026-12-01"],
    ["30 mei", "id", "2026-05-30"],
    ["17 agustus", "id", "2026-08-17"],
    ["today", "en", "2026-11-22"],
    ["Tomorrow", "en", "2026-11-23"],
    ["besok", "id", "2026-11-23"],
    ["lusa", "id", "2026-11-24"],
    ["kemarin", "id", "2026-11-21"],
    ["+3d", "en", "2026-11-25"],
    ["-2w", "en", "2026-11-08"],
    ["+1m", "en", "2026-12-22"],
    ["3 hari", "id", "2026-11-25"],
    ["2 minggu", "id", "2026-12-06"],
    ["next week", "en", "2026-11-29"],
    ["minggu depan", "id", "2026-11-29"],
    ["bulan depan", "id", "2026-12-22"],
    ["fri", "en", "2026-11-27"],
    ["next friday", "en", "2026-11-27"],
    ["sun", "en", "2026-11-29"],
    ["jumat", "id", "2026-11-27"],
    ["senin depan", "id", "2026-11-23"],
    ["soon", "en", null],
    ["", "en", null],
  ];
  it.each(cases)("%s (%s) → %s", (text, lang, want) => {
    expect(parseTyped(text, lang, today)).toBe(want);
  });
});
