// context-capsule — ui: sharing a capsule — what in it might be private, found with plain rules (no AI), and the copy
// you share with the items you chose replaced by "[removed]"

// Eight kinds (SPEC "Sharing a capsule"). The first four are the same rules as the old windows' "Before you share"
// (Swift ShareCheck in capture/Sources/Core.swift, Rust core::share_check): emails, phone numbers, the names in the
// "People, links and files" section, and links that look private. The share page adds money, companies, file paths and
// public links. spec/share-check.json holds the shared cases. Only the body is searched (front matter skipped); the
// copy changes the whole file, front matter too.

// What replaces each hidden item in the shared copy.
export const REMOVED = "[removed]";

// The kinds, in the order they are shown; `on`: hidden in the copy unless you say otherwise.
export const KINDS = [
  { kind: "people", title: "Names", one: "name", on: true },
  { kind: "emails", title: "Emails", one: "email", on: true },
  { kind: "phones", title: "Phone numbers", one: "phone number", on: true },
  { kind: "links", title: "Private links", one: "private link", on: true },
  { kind: "money", title: "Money", one: "amount of money", on: true },
  { kind: "companies", title: "Companies", one: "company", on: false },
  { kind: "files", title: "File paths", one: "file path", on: false },
  { kind: "public", title: "Public links", one: "public link", on: false },
];
const ORDER = KINDS.map((k) => k.kind);

// \w as the Mac's regexes see it (Unicode letters and digits, not just ASCII).
const WC = String.raw`\p{Alphabetic}\p{M}\p{Nd}\p{Pc}‌‍`;
const re = (p) => new RegExp(p, "gmu");

const link = re(String.raw`https?:\/\/[^\s)>\]]+`);
const email = re(String.raw`[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}`);
const phone = re(String.raw`(?<![${WC}+(])(?:\+|\()?\d[\d .()\-]*\d(?![${WC}])`);
const date = /\d{4}-\d{2}-\d{2}/;

// File paths: ~/…, /Users/…, /home/…, C:\… (up to a space or a quote/bracket; trailing ". , ; :" dropped).
const path = re(String.raw`(?<![${WC}\/~.\\])(?:~|\/Users|\/home|[A-Za-z]:)[\\\/][^\s${"`"}'"<>()\[\]{}|*]+`);

// Money: a currency sign or code and an amount ($12,000, ₹4,500, US$5, Rs. 300, EUR 40, $2 million, $12k), or an
// amount and a code (12k USD, 300 rupees). A bare number or a percentage is not money.
const AMOUNT = String.raw`\d{1,3}(?:,\d{2,3})+(?:\.\d+)?|\d+(?:\.\d+)?`;
const SCALE = String.raw`(?:\s?(?:k|K|M|bn|mn|million|billion|thousand|lakhs?|crores?)(?![\p{L}\p{N}]))?`;
const CODES = "USD|EUR|GBP|INR|AUD|CAD|SGD|AED|JPY|CNY|CHF|NZD";
const money = re(
  String.raw`(?:(?<![\p{L}\p{N}$])(?:US\$|A\$|C\$|S\$|NZ\$|HK\$|R\$|[$€£₹¥₩₽₺₦₱])\s?(?:${AMOUNT})${SCALE}`
  + String.raw`|(?<![\p{L}\p{N}])(?:Rs\.?|${CODES})\s?(?:${AMOUNT})${SCALE}`
  + String.raw`|(?<![\p{L}\p{N}.,$])(?:${AMOUNT})${SCALE}\s?(?:${CODES}|dollars?|euros?|rupees?)(?![\p{L}\p{N}]))`,
);

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
// A People bullet's role that says it is an organisation, not a person ("Acme — client company").
const orgRole = /\b(?:company|companies|firm|agency|vendor|supplier|organi[sz]ation|startup|business|employer|brand|bank|school|university|studio|corp)\b/i;

// The People section's bullets, each split into its name part and the rest (the role): [{ name, role, line }].
function peopleBullets(text) {
  const out = [];
  let inPeople = false;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("#")) {
      inPeople = line.startsWith("## ") && line.slice(3).trim().toLowerCase().startsWith("people");
      continue;
    }
    if (!inPeople || !(line.startsWith("- ") || line.startsWith("* "))) continue;
    let item = line.slice(2).replaceAll("**", "").replaceAll("__", "");
    let cut = item.length;
    for (const stop of [" — ", " – ", " - ", ":", "(", ",", ";", "<", " <"]) {
      const at = item.indexOf(stop);
      if (at >= 0 && at < cut) cut = at;
    }
    out.push({ name: item.slice(0, cut).trim(), role: item.slice(cut), line });
  }
  return out;
}

function looksLikeName(item) {
  const words = item.split(/\s+/).filter(Boolean);
  if (words.length < 1 || words.length > 4) return false;
  const first = words[0], last = words[words.length - 1];
  return nameWord.test(first) && nameWord.test(last)
    && words.every((w) => nameWord.test(w) || particles.has(w))
    && !words.some((w) => notNames.has(w.toLowerCase().replace(/\.$/, "")));
}

const hasLinkOrPath = (line) => /https?:\/\/|www\.|\S\/\S|~\/|[A-Za-z]:\\/.test(line);

// The name part of each bullet in the `## People…` section ("- Ana Ruiz — client", "- **Dev Raj** (engineer)"):
// 1–4 name words, on a line with no link or path, that isn't a thing like "Blog" or "GitHub profile".
export function bulletNames(text) {
  return peopleBullets(text).filter((b) => !hasLinkOrPath(b.line) && looksLikeName(b.name)).map((b) => b.name);
}

// Given names said on their own ("Ana said…" when the People section has "Ana Ruiz"): only when they appear on their own
// somewhere, are 3+ letters, and aren't also an ordinary word.
const commonWords = new Set(("will may june april august mark bill grace hope joy faith rose art max sky summer dawn "
  + "rich sunny lily daisy ruby pearl iris amber jade dean bob sue pat chase hunter page piper").split(" "));
function givenNames(text, names) {
  const out = [];
  for (const n of names) {
    const first = n.split(/\s+/)[0].replace(/\.$/, "");
    if (first === n || [...first].length < 3 || commonWords.has(first.toLowerCase()) || names.includes(first)) continue;
    if (alone(text, first, names)) out.push(first);
  }
  return out;
}

// True when `word` appears in text as a whole word outside every one of `longer`.
function alone(text, word, longer) {
  for (const seg of segments(text, [...longer, word])) if (seg.item === word) return true;
  return false;
}

// ---------- companies ----------

// One capitalised word of an organisation's name ("Acme", "AT&T", "O'Reilly"); not followed by digits ("Q3").
const ORG = String.raw`\p{Lu}[\p{L}\p{M}&'’\-]*(?![\p{L}\p{N}])`;
const SUFFIX = String.raw`(?:Inc\.?|Incorporated|Ltd\.?|Limited|LLC|L\.L\.C\.|LLP|PLC|plc|GmbH|Corp\.?|Corporation|Pvt\.?\s+Ltd\.?|Private\s+Limited|Pty\.?\s+Ltd\.?|S\.A\.|SAS|B\.V\.|AG)`;
const legal = re(String.raw`(?<![\p{L}\p{N}])((?:${ORG}\s+){0,3}${ORG})\s+(${SUFFIX})(?![\p{L}\p{N}])`);
const ROLE_WORDS = "client|customer|company|firm|agency|vendor|supplier|partner|employer|startup";
const afterRole = re(String.raw`(?<![\p{L}])(?:${ROLE_WORDS})s?:?\s+((?:${ORG}\s+){0,2}${ORG})`);
const beforeRole = re(String.raw`(?<![\p{L}\p{N}])((?:${ORG}\s+){0,2}${ORG}),?\s+\((?:the\s+|our\s+|a\s+)?(?:${ROLE_WORDS})\)`);
const atOrg = re(String.raw`(?<![\p{L}])(?:at|from)\s+((?:${ORG}\s+){0,2}${ORG})`);

// Names that are public tools and sites, not someone's company (and words that start a sentence).
const notCompanies = new Set([...notNames, ...("the a an this that these our their my your his her its at for from with "
  + "to by of and or in on new old mac macos windows linux apple microsoft amazon aws meta openai anthropic claude chatgpt "
  + "codex gemini cursor vscode code chrome safari firefox zen preview hashnode karya stripe paypal shopify whatsapp "
  + "telegram excel word powerpoint keynote pages numbers q1 q2 q3 q4 i we he she they monday tuesday wednesday thursday "
  + "friday saturday sunday january february march april may june july august september october november december "
  + "today tomorrow yesterday").split(" ")]);
// Words that end a phrase but aren't part of the organisation's name ("client Acme Report" → "Acme").
const generic = new Set(("report reports team teams project projects page pages deck account accounts call calls meeting "
  + "meetings proposal proposals website site app apps pricing contract contracts invoice invoices plan plans launch "
  + "review reviews brief draft drafts doc docs file files work pvt private pty").split(" "));

function orgName(words, startOfSentence) {
  let w = words.trim().split(/\s+/);
  while (w.length && notCompanies.has(w[0].toLowerCase())) w = w.slice(1);
  while (w.length && (generic.has(w[w.length - 1].toLowerCase()) || notCompanies.has(w[w.length - 1].toLowerCase()))) w = w.slice(0, -1);
  if (startOfSentence && w.length > 1) w = w.slice(1);
  if (!w.length || w.some((x) => notCompanies.has(x.toLowerCase()))) return null;
  const name = w.join(" ");
  return [...name].length >= 2 ? name : null;
}

function sentenceStart(text, at) {
  const before = text.slice(0, at).replace(/[*_\s]+$/, "");
  return before === "" || /[.!?:\n]$/.test(before) || /(?:^|\n)\s*(?:[-*+]|\d+[.)])$/.test(before);
}

// Organisations: "Acme Inc / Ltd / LLC / Pvt Ltd…" (and "Acme" on its own), a capitalised name next to "client" or
// "company" ("client Acme", "Globex (client)"), the role text of a People bullet ("head of sales at Acme"), and a People
// bullet that is an organisation ("Acme — client company"). Never public tools and sites (GitHub, Figma, Medium…).
function companies(blank, original, people) {
  const out = [];
  const add = (n) => { if (n && !people.includes(n) && !out.includes(n)) out.push(n); };
  for (const m of blank.matchAll(legal)) {
    const lead = orgName(m[1], sentenceStart(blank, m.index));
    if (!lead) continue;
    // The full name as written ("Acme Inc"), then the name on its own ("Acme").
    add(m[0].slice(m[0].lastIndexOf(lead)));
    add(lead);
  }
  for (const r of [afterRole, beforeRole]) {
    for (const m of blank.matchAll(r)) add(orgName(m[1], false));
  }
  for (const b of peopleBullets(original)) {
    if (orgRole.test(b.role) && !hasLinkOrPath(b.line) && looksLikeName(b.name)) add(b.name);
    if (!looksLikeName(b.name)) continue;
    for (const m of b.role.matchAll(atOrg)) add(orgName(m[1], false));
  }
  return out;
}

// ---------- all of it ----------

// What each kind holds, every item once in order of first appearance in the body: { people: [...], emails: [...], … }.
function lists(text) {
  const original = body(text);
  const s = { text: original };
  const allLinks = take(link, s, true, true);
  const links = allLinks.filter(isPrivateLink);
  const pub = allLinks.filter((u) => !isPrivateLink(u));
  const emails = take(email, s, true);
  const files = take(path, s, true, true);
  const amounts = take(money, s, true, true);
  const phones = take(phone, s, false).filter((p) => {
    const digits = (p.match(/\p{N}/gu) || []).length;
    return digits >= 8 && digits <= 15 && !date.test(p);
  });
  const bullets = peopleBullets(original);
  let people = bulletNames(original).filter((n) => !bullets.some((b) => b.name === n && orgRole.test(b.role)));
  people = [...people, ...givenNames(original, people)];
  const orgs = companies(s.text, original, people);

  const ordered = (items) => {
    const at = (x) => firstAt(original, x);
    return [...new Set(items)].map((x, i) => [at(x), i, x]).sort((a, b) => a[0] - b[0] || a[1] - b[1]).map((e) => e[2]);
  };
  // Found left to right already, except names and companies.
  const raw = { people: ordered(people), emails, phones, links, money: amounts, companies: ordered(orgs), files, public: pub };
  const out = {};
  const seen = new Set();
  for (const k of ORDER) {
    out[k] = [...new Set(raw[k])].filter((x) => !seen.has(x));
    out[k].forEach((x) => seen.add(x));
  }
  return out;
}

// All eight kinds, in order: [{ kind, title, on, items }]; each lists its items once, in order of first appearance.
export function find(text) {
  const l = lists(text);
  return KINDS.map(({ kind, title, on }) => ({ kind, title, on, items: l[kind] }));
}

// Only the kinds with something in them.
export function nonEmpty(groups) {
  return groups.filter((g) => g.items.length > 0);
}

// ---------- where they are, and the copy ----------

const wordy = (c) => c !== undefined && /[\p{L}\p{N}_]/u.test(c);
function charBefore(text, i) {
  if (i <= 0) return undefined;
  const low = /[\uDC00-\uDFFF]/.test(text[i - 1]) && i > 1;
  return String.fromCodePoint(text.codePointAt(i - (low ? 2 : 1)));
}

// The text cut into pieces: [{ text, item? }], `item` set where one of `items` is. Left to right, the longest item
// first at each place, so a short item never matches inside a longer one; an item that starts or ends with a letter or
// digit only counts as a whole word ("Ana" is not inside "Analysis").
export function segments(text, items) {
  const list = [...new Set(items)].filter(Boolean)
    .sort((a, b) => [...b].length - [...a].length || (a < b ? 1 : a > b ? -1 : 0));
  const out = [];
  let plain = "";
  let i = 0;
  scan: while (i < text.length) {
    for (const item of list) {
      if (!text.startsWith(item, i)) continue;
      const end = i + item.length;
      if (wordy(item[0]) && wordy(charBefore(text, i))) continue;
      if (wordy(item[item.length - 1]) && wordy(text[end])) continue;
      if (plain) out.push({ text: plain });
      plain = "";
      out.push({ text: item, item });
      i = end;
      continue scan;
    }
    const c = String.fromCodePoint(text.codePointAt(i));
    plain += c;
    i += c.length;
  }
  if (plain) out.push({ text: plain });
  return out;
}

// Where `item` first is in text as a whole word (Infinity if it isn't).
function firstAt(text, item) {
  for (let i = text.indexOf(item); i >= 0; i = text.indexOf(item, i + 1)) {
    if (wordy(item[0]) && wordy(charBefore(text, i))) continue;
    if (wordy(item[item.length - 1]) && wordy(text[i + item.length])) continue;
    return i;
  }
  return Infinity;
}

// Every place an item is in the body: [{ kind, text, start, end }] (offsets into the whole text, front matter included).
export function findings(text) {
  const b = body(text);
  const offset = text.length - b.length;
  const kindOf = new Map();
  for (const g of find(text)) for (const x of g.items) kindOf.set(x, g.kind);
  const out = [];
  let at = offset;
  for (const seg of segments(b, [...kindOf.keys()])) {
    if (seg.item) out.push({ kind: kindOf.get(seg.item), text: seg.item, start: at, end: at + seg.text.length });
    at += seg.text.length;
  }
  return out;
}

// The copy to share: the whole file with each of `hidden` replaced by "[removed]" wherever the share page shows it
// (`items`: everything found, so a hidden item inside a longer kept one stays, as the page shows it).
export function shareCopy(text, items, hidden) {
  const h = hidden instanceof Set ? hidden : new Set(hidden);
  return segments(text, items).map((s) => (s.item && h.has(s.item) ? REMOVED : s.text)).join("");
}

// A copy with every occurrence of each item replaced by "[removed]" (the whole file, front matter too).
export function remove(text, items) {
  return shareCopy(text, items, items);
}
