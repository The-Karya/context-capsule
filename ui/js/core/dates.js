// context-capsule — ui: dates and times for people, in the page's language and 12/24-hour style (DayWords in
// capture/Sources/Core.swift), plus the "YYYY-MM-DD" day keys the pages pass around (local days).

const pad = (n, w = 2) => String(n).padStart(w, "0");

/** "YYYY-MM-DD" of the local day. */
export function dayKey(ms) {
  const d = new Date(ms);
  return `${pad(d.getFullYear(), 4)}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** "YYYY-MM-DD" -> local midnight (ms); null when it isn't one. */
export function parseKey(ymd) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(ymd));
  if (!m) return null;
  const d = new Date(+m[1], +m[2] - 1, +m[3]);
  return d.getMonth() === +m[2] - 1 ? d.getTime() : null;
}

/** The day `n` days after (or before) `ymd`. */
export function addDays(ymd, n) {
  const t = parseKey(ymd);
  if (t == null) return ymd;
  const d = new Date(t);
  return dayKey(new Date(d.getFullYear(), d.getMonth(), d.getDate() + n).getTime());
}

/** Local midnight of that moment's day. */
export function startOfDay(ms) {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** Formatters for one locale. `hour12`: true / false, or undefined for the locale's own. */
export function makeDates({ locale, hour12 } = {}) {
  const fmt = (o) => new Intl.DateTimeFormat(locale, hour12 === undefined ? o : { ...o, hour12 });
  const longF = fmt({ weekday: "long", day: "numeric", month: "long" });
  const longY = fmt({ weekday: "long", day: "numeric", month: "long", year: "numeric" });
  // The locale's own short time ("14:05", "2:05 PM"); forced 12 h: "2:05 pm"; forced 24 h: "07:05".
  const timeF =
    hour12 === undefined
      ? new Intl.DateTimeFormat(locale, { timeStyle: "short" })
      : hour12
        ? new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit", hour12: true })
        : new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  const hourF = hour12 === false ? new Intl.DateTimeFormat(locale, { hour: "2-digit", hourCycle: "h23" }) : fmt({ hour: "numeric" });
  const monthF = new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" });
  const shortF = new Intl.DateTimeFormat(locale, { day: "numeric", month: "short" });
  const narrow = new Intl.DateTimeFormat(locale, { weekday: "narrow" });
  const weekdayF = new Intl.DateTimeFormat(locale, { weekday: "short" });
  const shortY = new Intl.DateTimeFormat(locale, { day: "numeric", month: "short", year: "numeric" });

  const time = (ms) => timeF.format(ms);
  return {
    /** "Friday, October 23" / "Friday 23 October"; the year only when it isn't this year. */
    long(ms, nowMs = Date.now()) {
      return (new Date(ms).getFullYear() === new Date(nowMs).getFullYear() ? longF : longY).format(ms);
    },
    /** "2:05 PM" / "14:05". */
    time,
    /** "x – y"; one time when both read the same. */
    span(a, b) {
      const x = time(a);
      const y = time(b);
      return x === y ? x : `${x} – ${y}`;
    },
    /** The day bar's hour labels: "12 PM" / "14". */
    hour: (ms) => hourF.format(ms),
    /** "October 2026". `month`: 1–12. */
    monthTitle: (year, month) => monthF.format(new Date(year, month - 1, 1)),
    /** The calendar's weekday heads, very short, starting at `firstWeekday` (1 = Sunday). */
    weekdays(firstWeekday = 1) {
      const out = [];
      // 2023-01-01 was a Sunday.
      for (let i = 0; i < 7; i++) out.push(narrow.format(new Date(2023, 0, 1 + ((firstWeekday - 1 + i) % 7))));
      return out;
    },
    /** A month's cells: blanks (null) before the 1st so it starts on the right weekday, then 1…n. */
    monthCells(year, month, firstWeekday = 1) {
      const lead = (new Date(year, month - 1, 1).getDay() + 1 - firstWeekday + 7) % 7;
      const days = new Date(year, month, 0).getDate();
      return [...Array(lead).fill(null), ...Array.from({ length: days }, (_, i) => i + 1)];
    },
    /** "Today" / "Yesterday" / "Oct 22" for a "YYYY-MM-DD" day. */
    shortDay(ymd, nowMs = Date.now()) {
      const t = parseKey(ymd);
      if (t == null) return String(ymd);
      const today = dayKey(nowMs);
      if (ymd === today) return "Today";
      if (ymd === addDays(today, -1)) return "Yesterday";
      return shortF.format(t);
    },
    /** The Capsules page's date gutter for a "YYYY-MM-DD" day: { top, sub } — "Today" / "Oct 23", "Thu" / "Oct 22"
     * within the week, else "Oct 12" / "Mon" ("Oct 12, 2025" in another year). */
    gutter(ymd, nowMs = Date.now()) {
      const t = parseKey(ymd);
      if (t == null) return { top: String(ymd || ""), sub: "" };
      const today = dayKey(nowMs);
      const date = shortF.format(t);
      if (ymd === today) return { top: "Today", sub: date };
      if (ymd === addDays(today, -1)) return { top: "Yesterday", sub: date };
      if (ymd < today && ymd > addDays(today, -7)) return { top: weekdayF.format(t), sub: date };
      const thisYear = new Date(t).getFullYear() === new Date(nowMs).getFullYear();
      return { top: (thisYear ? shortF : shortY).format(t), sub: weekdayF.format(t) };
    },
  };
}
