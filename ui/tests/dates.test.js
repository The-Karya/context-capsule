// context-capsule — ui: tests for dates and times in en-US and en-GB, 12 and 24 hours, and the day keys.

import { beforeAll, expect, test } from "bun:test";
import { makeDates, dayKey, parseKey, addDays, startOfDay } from "../js/core/dates.js";

beforeAll(() => {
  process.env.TZ = "Etc/GMT-2";
});

const at = (h, m = 0) => new Date(2026, 9, 23, h, m).getTime();
const NOW = at(10);
// Some ICU versions put a narrow no-break space before AM/PM.
const plain = (s) => s.replace(/[  ]/g, " ");

test("en-US", () => {
  const d = makeDates({ locale: "en-US" });
  expect(d.long(NOW, NOW)).toBe("Friday, October 23");
  expect(d.long(new Date(2025, 11, 31).getTime(), NOW)).toBe("Wednesday, December 31, 2025");
  expect(plain(d.time(at(14, 5)))).toBe("2:05 PM");
  expect(plain(d.span(at(14, 5), at(15, 0)))).toBe("2:05 PM – 3:00 PM");
  expect(plain(d.span(at(14, 5), at(14, 5) + 30_000))).toBe("2:05 PM");
  expect(plain(d.hour(at(12)))).toBe("12 PM");
  expect(d.monthTitle(2026, 10)).toBe("October 2026");
  expect(d.weekdays(1)).toEqual(["S", "M", "T", "W", "T", "F", "S"]);
  expect(d.weekdays(2)).toEqual(["M", "T", "W", "T", "F", "S", "S"]);
  expect(d.shortDay("2026-10-23", NOW)).toBe("Today");
  expect(d.shortDay("2026-10-22", NOW)).toBe("Yesterday");
  expect(d.shortDay("2026-10-21", NOW)).toBe("Oct 21");
  const h24 = makeDates({ locale: "en-US", hour12: false });
  expect(h24.time(at(14, 5))).toBe("14:05");
  expect(h24.time(at(9, 5))).toBe("09:05");
  expect(h24.hour(at(9))).toBe("09");
});

test("en-GB", () => {
  const d = makeDates({ locale: "en-GB" });
  expect(d.long(NOW, NOW)).toBe("Friday 23 October");
  expect(d.long(new Date(2025, 11, 31).getTime(), NOW)).toMatch(/^Wednesday,? 31 December 2025$/); // ICU adds a comma the Mac leaves out
  expect(d.time(at(14, 5))).toBe("14:05");
  expect(d.time(at(9, 5))).toBe("09:05");
  expect(d.hour(at(14))).toBe("14");
  expect(d.shortDay("2026-10-21", NOW)).toBe("21 Oct");
  const h12 = makeDates({ locale: "en-GB", hour12: true });
  expect(plain(h12.time(at(14, 5))).toLowerCase()).toBe("2:05 pm");
});

test("month cells", () => {
  const d = makeDates({ locale: "en-GB" });
  // October 2026 starts on a Thursday.
  const mon = d.monthCells(2026, 10, 2);
  expect(mon.slice(0, 4)).toEqual([null, null, null, 1]);
  expect(mon.length).toBe(3 + 31);
  expect(d.monthCells(2026, 10, 1).slice(0, 5)).toEqual([null, null, null, null, 1]);
  expect(d.monthCells(2027, 2, 2)).toEqual(Array.from({ length: 28 }, (_, i) => i + 1));
});

test("day keys", () => {
  expect(dayKey(at(23, 59))).toBe("2026-10-23");
  expect(parseKey("2026-10-23")).toBe(new Date(2026, 9, 23).getTime());
  expect(parseKey("2026-02-30")).toBeNull();
  expect(parseKey("nope")).toBeNull();
  expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
  expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
  expect(startOfDay(at(15))).toBe(new Date(2026, 9, 23).getTime());
});
