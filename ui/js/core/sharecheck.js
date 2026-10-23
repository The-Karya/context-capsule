// context-capsule — ui: "Before you share" — what in a capsule might be private, found with plain rules (no AI)

// Same rules as the Mac's ShareCheck (capture/Sources/Core.swift) and Rust core::share_check; spec/share-check.json
// holds the shared cases. Only four kinds of thing are flagged: email addresses, phone numbers, the names in the
// "People, links and files" section, and links that look private. Not capitalised words, not ordinary public links.
// Only the body is searched (front matter skipped); remove() changes the whole file, front matter too.

// What replaces each checked item in the shared copy.
export const REMOVED = "[removed]";

// The groups, in the order they are shown.
export const KINDS = [
  { kind: "people", title: "People" },
  { kind: "emails", title: "Emails" },
  { kind: "phones", title: "Phone numbers" },
  { kind: "links", title: "Private links" },
];

// \w as the Mac's regexes see it (Unicode letters and digits, not just ASCII).
const WC = String.raw`\p{Alphabetic}\p{M}\p{Nd}\p{Pc}‌‍`;
const re = (p) => new RegExp(p, "gmu");

const link = re(String.raw`https?:\/\/[^\s)>\]]+`);
const email = re(String.raw`[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}`);
const phone = re(String.raw`(?<![${WC}+(])(?:\+|\()?\d[\d .()\-]*\d(?![${WC}])`);
const date = /\d{4}-\d{2}-\d{2}/;

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

// ---------- links that look private ----------

// Query keys that carry a secret or point at one private thing.
const secretKey = /^(?:token|access_token|id_token|refresh_token|auth|authuser|key|api_?key|apikey|secret|sig|signature|password|pass|pwd|session|sessionid|sid|code|otp|invite|share|sharing|usp|x-amz-[a-z-]+|id|uid|userid|docid|fileid|[a-z]+_id)$/i;
// Hosts that only make sense on someone's own computer or network.
const internalHost = /^(?:localhost|127\.\d+\.\d+\.\d+|0\.0\.0\.0|10\.\d+\.\d+\.\d+|192\.168\.\d+\.\d+|172\.(?:1[6-9]|2\d|3[01])\.\d+\.\d+|\[::1\]|[^.]+|.+\.(?:local|internal|intranet|corp|lan|home|localhost))$/i;
// Share links to private documents, meetings and files: host, then the start of the path.
const shareLinks = [
  [/^docs\.google\.com$/i, /^\/(?:document|spreadsheets|presentation|forms|drawings)\/(?:u\/\d+\/)?d\//],
  [/^drive\.google\.com$/i, /^\/(?:file\/d\/|drive\/folders\/|drive\/u\/|open)/],
  [/^meet\.google\.com$/i, /^\/[a-z]/],
  [/(?:^|\.)zoom\.us$/i, /^\/(?:j|my|s|rec)\//],
  [/^teams\.microsoft\.com$/i, /^\/l\//],
  [/(?:^|\.)dropbox\.com$/i, /^\/(?:s|sh|scl)\//],
  [/^1drv\.ms$/i, /^\//],
  [/^onedrive\.live\.com$/i, /^\//],
  [/\.sharepoint\.com$/i, /^\//],
  [/(?:^|\.)figma\.com$/i, /^\/(?:file|design|proto|board|deck)\//],
  [/(?:^|\.)notion\.so$/i, /^\//],
  [/^app\.box\.com$/i, /^\/s\//],
  [/(?:^|\.)wetransfer\.com$/i, /^\/downloads\//],
];

// True when a link looks private: a token, key or id in its query, a local or internal host, a password in it, or
// a share link to a private document, meeting or file. Ordinary public links (a blog, a homepage) are not.
export function isPrivateLink(url) {
  const m = /^https?:\/\/([^/?#]*)([^?#]*)(?:\?([^#]*))?/i.exec(url);
  if (!m) return false;
  let host = m[1];
  if (host.includes("@")) return true; // user:password@host
  host = host.replace(/:\d+$/, "").toLowerCase();
  const path = m[2] || "/";
  const query = m[3] || "";
  if (internalHost.test(host)) return true;
  if (shareLinks.some(([h, p]) => h.test(host) && p.test(path))) return true;
  for (const pair of query.split("&")) {
    if (!pair) continue;
    const [k, v = ""] = pair.split("=");
    if (secretKey.test(k) && v) return true;
    if (v.length >= 20 && /[A-Za-z]/.test(v) && /\d/.test(v) && /^[A-Za-z0-9_\-.%~+/=]+$/.test(v)) return true; // looks random
  }
  return false;
}

// ---------- names in "People, links and files" ----------

// Words that start a People bullet but aren't a person (things people list there: places, sites, tools).
const notNames = new Set(
  ("blog post posts site website page link links repo repos repository docs doc document documents file files folder "
    + "profile account email emails phone github gitlab bitbucket slack figma notion google drive dropbox jira linear "
    + "trello asana discord twitter linkedin youtube instagram facebook medium hashnode substack wikipedia reddit "
    + "zoom teams meet calendar spreadsheet sheet sheets slides deck notes note project projects team client clients "
    + "company app apps tool tools dataset datasets paper papers article articles course video videos me myself you "
    + "they we us them none other others").split(" "),
);
// Small words that can sit inside a name ("Ana de la Cruz", "Ludwig van Beethoven").
const particles = new Set("de la le da das do dos del della di du van von der den ter bin binti al el y e".split(" "));
// One name word: a capital then lower-case letters ("Ana", "O'Neil", "Ruiz-Lopez", "McKay"); not "GitHub", "API", "ML".
const nameWord = /^(?:\p{Lu}['’])?(?:Mc|Mac)?\p{Lu}\p{Ll}+(?:[-'’]\p{Lu}?\p{Ll}+)*\.?$/u;

// The name part of each bullet in the `## People…` section ("- Ana Ruiz — client", "- **Dev Raj** (engineer)"):
// 1–4 name words, on a line with no link or path, that isn't a thing like "Blog" or "GitHub profile".
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
    if (/https?:\/\/|www\.|\S\/\S|~\/|[A-Za-z]:\\/.test(line)) continue; // a link or a file, not a person
    let item = line.slice(2).replaceAll("**", "").replaceAll("__", "");
    for (const stop of [" — ", " – ", " - ", ":", "(", ",", ";", "<", " <"]) {
      const at = item.indexOf(stop);
      if (at >= 0) item = item.slice(0, at);
    }
    item = item.trim();
    const words = item.split(/\s+/).filter(Boolean);
    if (words.length < 1 || words.length > 4) continue;
    const first = words[0], last = words[words.length - 1];
    const ok = nameWord.test(first) && nameWord.test(last)
      && words.every((w) => nameWord.test(w) || particles.has(w))
      && !words.some((w) => notNames.has(w.toLowerCase().replace(/\.$/, "")));
    if (ok) names.push(item);
  }
  return names;
}

// ---------- all of it ----------

// All four groups, in order: [{ kind, title, items }]; each lists its items once, in order of first appearance.
export function find(text) {
  const original = body(text);
  const s = { text: original };
  const links = take(link, s, true, true).filter(isPrivateLink);
  const emails = take(email, s, true);
  const phones = take(phone, s, false).filter((p) => {
    const digits = (p.match(/\p{N}/gu) || []).length;
    return digits >= 8 && digits <= 15 && !date.test(p);
  });
  const people = bulletNames(original);

  const ordered = (items) => {
    const at = (x) => { const i = original.indexOf(x); return i < 0 ? Infinity : i; };
    return [...new Set(items)].map((x, i) => [at(x), i, x]).sort((a, b) => a[0] - b[0] || a[1] - b[1]).map((e) => e[2]);
  };
  const lists = { people, emails, phones, links };
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
