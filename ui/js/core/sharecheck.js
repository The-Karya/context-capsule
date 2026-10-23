// context-capsule — ui: "Check before sharing" — what in a capsule might be private, found with plain rules (no AI)

// A port of the Mac's ShareCheck (capture/Sources/Core.swift); spec/share-check.json holds the shared cases.
// Only the body is searched (front matter skipped); remove() changes the whole file, front matter too.

// What replaces each checked item in the shared copy.
export const REMOVED = "[removed]";

// The groups, in the order they are shown.
export const KINDS = [
  { kind: "people", title: "People" },
  { kind: "emails", title: "Emails" },
  { kind: "phones", title: "Phone numbers" },
  { kind: "links", title: "Links" },
  { kind: "money", title: "Money" },
  { kind: "files", title: "Files" },
];

// \w as the Mac's regexes see it (Unicode letters and digits, not just ASCII).
const W = String.raw`[\p{Alphabetic}\p{M}\p{Nd}\p{Pc}‌‍]`;
const re = (p) => new RegExp(p, "gmu");

const link = re(String.raw`https?:\/\/[^\s)>\]]+`);
const email = re(String.raw`[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}`);
const file = re(String.raw`(?:^|(?<=[\s(]))(?:~\/[^\s\/]\S*|\/[^\s\/]+\/[^\s\/]\S*|[A-Za-z]:\\\S+)`);
const phone = re(String.raw`(?<![${W.slice(1, -1)}+])[+\d][\d .()\-]*\d(?!${W})`);
const date = /\d{4}-\d{2}-\d{2}/;
const amount = String.raw`\d+(?:,\d{3})*(?:\.\d+)?`;
const moneySign = re(String.raw`[$€£₹] ?` + amount + String.raw`(?:(?: ?(?:million|billion|bn)|[kKmMbB])(?!${W}))?`);
const moneyWord = re(String.raw`(?<![${W.slice(1, -1)}.,])` + amount + String.raw` ?(?:USD|EUR|GBP|INR|dollars|euros|pounds|rupees)(?!${W})`);
const handle = re(String.raw`(?<![${W.slice(1, -1)}@])@[A-Za-z0-9_]+`);

// The text after a leading `---` front matter block (the whole text if there is none).
export function body(text) {
  const crlf = text.startsWith("---\r\n");
  if (!crlf && !text.startsWith("---\n")) return text;
  const close = /^---[ \t]*\r?$/m;
  const m = close.exec(text.slice(crlf ? 5 : 4));
  if (!m) return text;
  return text.slice((crlf ? 5 : 4) + m.index + m[0].length);
}

// Every match of `r` in s.text (trailing ". , ; :" dropped when `trim`); blanked out (same length) when `blank`.
function take(r, s, blank, trim = false) {
  const found = [];
  const ranges = [];
  for (const m of s.text.matchAll(r)) {
    let t = m[0];
    while (trim && t.length > 0 && ".,;:".includes(t[t.length - 1])) t = t.slice(0, -1);
    if (!t) continue;
    found.push(t);
    ranges.push([m.index, t.length]);
  }
  if (blank) {
    for (const [at, n] of ranges.reverse()) s.text = s.text.slice(0, at) + " ".repeat(n) + s.text.slice(at + n);
  }
  return found;
}

// The start of each bullet in the `## People…` section that looks like a name (1–4 capitalised words).
export function bulletNames(text) {
  const names = [];
  let inPeople = false;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("#")) {
      inPeople = line.startsWith("## ") && line.slice(3).trim().toLowerCase().startsWith("people");
      continue;
    }
    if (!inPeople || !(line.startsWith("- ") || line.startsWith("* "))) continue;
    let item = line.slice(2).replaceAll("**", "");
    for (const stop of [" — ", " – ", " - ", ":", "(", ","]) {
      const at = item.indexOf(stop);
      if (at >= 0) item = item.slice(0, at);
    }
    item = item.trim();
    const words = item.split(" ").filter(Boolean);
    const ok = words.length >= 1 && words.length <= 4
      && words.every((w) => /^\p{Lu}/u.test(w))
      && !/[\p{N}\/@]/u.test(item)
      && !item.toLowerCase().includes("http");
    if (ok) names.push(item);
  }
  return names;
}

// All six groups, in order: [{ kind, title, items }]; each lists its items once, in order of first appearance.
export function find(text) {
  const original = body(text);
  const s = { text: original };
  const links = take(link, s, true, true);
  const emails = take(email, s, true);
  const files = take(file, s, false, true);
  const phones = take(phone, s, false).filter((p) => {
    const digits = (p.match(/\p{N}/gu) || []).length;
    return digits >= 8 && digits <= 15 && !date.test(p);
  });
  const money = [...take(moneySign, s, false), ...take(moneyWord, s, false)];
  const people = [...take(handle, s, false), ...bulletNames(original)];

  const ordered = (items) => {
    const at = (x) => { const i = original.indexOf(x); return i < 0 ? Infinity : i; };
    return [...new Set(items)].map((x, i) => [at(x), i, x]).sort((a, b) => a[0] - b[0] || a[1] - b[1]).map((e) => e[2]);
  };
  const lists = { people, emails, phones, links, money, files };
  return KINDS.map(({ kind, title }) => ({ kind, title, items: ordered(lists[kind]) }));
}

// Only the groups with something in them (empty: nothing to check, share straight away).
export function nonEmpty(groups) {
  return groups.filter((g) => g.items.length > 0);
}

// A copy with every occurrence of each item (longest first) replaced by "[removed]". One pass, left to right,
// so a short item never matches inside a "[removed]" put there for a longer one. The whole file, front matter too.
export function remove(text, items) {
  const list = [...new Set(items)].filter(Boolean)
    .sort((a, b) => [...b].length - [...a].length || (a < b ? 1 : a > b ? -1 : 0));
  if (!list.length) return text;
  let out = "";
  let i = 0;
  scan: while (i < text.length) {
    for (const item of list) {
      if (text.startsWith(item, i)) {
        out += REMOVED;
        i += item.length;
        continue scan;
      }
    }
    const c = String.fromCodePoint(text.codePointAt(i));
    out += c;
    i += c.length;
  }
  return out;
}
