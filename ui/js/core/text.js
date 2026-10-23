// context-capsule — ui: small text helpers shared by the core modules — letters as people see them, trimming,
// search folding, byte counts, ISO times as the recorders write them.

const segmenter = typeof Intl !== "undefined" && Intl.Segmenter ? new Intl.Segmenter(undefined, { granularity: "grapheme" }) : null;

/** The letters as seen (grapheme clusters), like Swift's String.count. */
export function graphemes(s) {
  return segmenter ? Array.from(segmenter.segment(s), (x) => x.segment) : Array.from(s);
}

/** Spaces and tabs off both ends (not newlines): Swift's `.whitespaces`. */
export function trimSpaces(s) {
  return String(s).replace(/^[\p{Zs}\t]+|[\p{Zs}\t]+$/gu, "");
}

/** Lower case, without accents, full-width letters as plain ones: what search compares. */
export function searchable(s) {
  return String(s).normalize("NFKD").replace(/\p{M}/gu, "").toLowerCase();
}

/** How many bytes the text takes in UTF-8. */
export function utf8Length(s) {
  let n = 0;
  for (const ch of String(s)) {
    const c = ch.codePointAt(0);
    n += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
  }
  return n;
}

/** Lines split like Swift's `isNewline` (\n, \r\n, \r, U+0085, U+2028, U+2029, \v, \f). */
export function lines(s) {
  return String(s).split(/\r\n|[\n\r\u0085\u2028\u2029\u000B\u000C]/);
}

/** "2026-10-19T10:00:00Z" (or an offset), whole seconds only like Foundation; null when it isn't one. */
export function parseISO(s) {
  if (typeof s !== "string") return null;
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(Z|[+-]\d{2}:\d{2})$/.test(s)) return null;
  const t = Date.parse(s);
  return Number.isNaN(t) ? null : t;
}

/** One JSON object per line; broken lines and non-objects skipped. Yields [index, line, obj]. */
export function* jsonLines(text) {
  if (!text) return;
  const all = String(text).split("\n");
  for (let i = 0; i < all.length; i++) {
    const line = all[i];
    if (!line.trim()) continue;
    let obj;
    try {
      obj = JSON.parse(line);
    } catch {
      continue;
    }
    if (obj && typeof obj === "object" && !Array.isArray(obj)) yield [i, line, obj];
  }
}

/** A string field, "" when missing or not a string. */
export function str(obj, key) {
  const v = obj[key];
  return typeof v === "string" ? v : "";
}
