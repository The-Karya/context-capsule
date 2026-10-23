// context-capsule — ui: tests for the Timeline, mirroring capture-rs/core/tests/timeline.rs — a recording across
// midnight, "Looked at", notes, gaps, a correction, a recording still going — then every stretch rule, the summary,
// the day bar, "Where your time went", search, folding, the delete words, and choosing the day.

import { beforeAll, expect, test } from "bun:test";
import * as T from "../js/core/timeline.js";
import { makeDates, addDays } from "../js/core/dates.js";

// The fixtures' local time: two hours east of UTC.
// Formatters are made after the zone is set (Intl keeps the zone it was made in).
let D;
beforeAll(() => {
  process.env.TZ = "Etc/GMT-2";
  D = makeDates({ locale: "en-GB", hour12: false });
});

const span = (a, b) => D.span(a, b);

const lines = (...l) => l.map((x) => x + "\n").join("");
const A_EVENTS = lines(
  `{"t":"2026-10-19T21:15:03Z","end":"2026-10-19T21:30:00Z","app":"Google Chrome","bundle":"chrome.exe","title":"Pricing — Linear","where":"linear.app/acme/pricing","project":"web","project_by":"auto","src":"ax","text":"Pricing\\nFree\\nTeams $12\\nEnterprise\\nContact us"}`,
  `{"t":"2026-10-19T21:40:00Z","end":"2026-10-19T21:40:00Z","app":"Figma","bundle":"figma.exe","title":"Pricing v2","focus":"pointer","src":"ocr","text":"Annual −20%"}`,
  `{"t":"2026-10-19T21:59:30Z","end":"2026-10-19T21:59:50Z","kind":"voice","src":"voice","app":"Figma","bundle":"figma.exe","title":"Pricing v2","project":"web","project_by":"auto","text":"We keep annual at twenty percent off because churn drops."}`,
  `{"t":"2026-10-19T22:05:00Z","end":"2026-10-19T22:20:00Z","app":"Code","bundle":"code.exe","title":"main.rs — capsule","where":"C:\\\\src\\\\capsule\\\\main.rs","src":"ax","text":"fn main() {}"}`,
);
const A_GAPS = lines(
  `{"t":"2026-10-19T21:45:00Z","end":"2026-10-19T21:55:00Z","why":"private_app"}`,
  `{"t":"2026-10-19T22:10:00Z","end":"2026-10-19T22:14:30Z","why":"locked"}`,
  "not json",
  `{"t":"2026-10-19T22:16:00Z","end":"2026-10-19T22:17:00Z","why":"new_reason"}`,
);
const B_EVENTS = lines(
  `{"t":"2026-10-20T07:30:03Z","end":"2026-10-20T08:00:00Z","app":"Slack","bundle":"slack.exe","title":"#design — Acme","src":"ax","text":"Ana: can we ship Friday?"}`,
  `{"t":"2026-10-20T08:01:00Z","end":"2026-10-20T08:01:00Z","kind":"note","src":"typed","app":"","bundle":"","title":"","text":"Decided: ship Friday, without annual."}`,
  `{"t":"2026-10-20T08:02:00Z","end":"2026-10-20T08:10:00Z","app":"Google Chrome","bundle":"chrome.exe","title":"Acme report","where":"docs.google.com/document/d/1AbC","src":"ax","text":"Q3 report"}`,
);
const C_EVENTS = lines(
  `{"t":"2026-10-20T12:00:03Z","end":"2026-10-20T12:29:00Z","app":"Code","bundle":"code.exe","title":"timeline.rs — capsule","src":"ax","text":"pub fn load_day"}`,
);

const FILES = {
  "20261019-231500": {
    session: `{"id":"20261019-231500","started_at":"2026-10-19T21:15:00Z","ended_at":"2026-10-19T22:20:01Z"}`,
    events: A_EVENTS,
    gaps: A_GAPS,
  },
  "20261020-093000": {
    session: `{"id":"20261020-093000","ended_at":"2026-10-20T08:10:02Z"}`,
    events: B_EVENTS,
    assign: `{"assign":[{"from":"2026-10-20T07:00:00Z","to":"2026-10-20T08:01:30Z","project":"app"}]}`,
    gaps: lines(`{"t":"2026-10-20T08:00:10Z","end":"2026-10-20T08:00:50Z","why":"not_shared"}`),
  },
  "20261020-140000": { session: `{"ended_at":null}`, events: C_EVENTS },
  notes: { events: A_EVENTS },
  "20261023-090000": {
    events: lines(`{"t":"2026-10-23T07:00:03Z","end":"2026-10-23T07:00:03Z","app":"Code","bundle":"code.exe","title":"x","src":"ax","text":""}`),
  },
};

const PROJECTS = [
  { id: "web", name: "Website", match: ["linear.app"] },
  { id: "app", name: "Mobile app", match: [] },
  { id: "report", name: "Client report", match: ["docs.google.com/document/d/1abc"] },
];
const nameOf = (id) => PROJECTS.find((p) => p.id === id)?.name ?? id;

const day = (key, files = FILES, projects = PROJECTS) => T.loadDay(key, files, { projects, recording: "20261020-140000" });

/** Rust's Block::line: projects by name, else the apps. */
function line(b) {
  const n = T.names(b);
  return T.blockLine(b, span(b.start, b.end), n.byProject ? n.keys.map(nameOf) : n.keys);
}

/** What an opened recording lists, one line each. */
function kinds(b) {
  return b.entries.map((e) =>
    e.type === "stretch"
      ? `${span(e.stretch.start, e.stretch.end)} ${e.stretch.name} · ${T.readsLine(e.stretch)}`
      : e.type === "note"
        ? `note ${span(e.moment.t, e.moment.end)}`
        : T.gapLine(e.gap, span(e.gap.t, e.gap.end)),
  );
}

/** Rust's time_went: names, lengths, colours. */
function went(d, projects = PROJECTS) {
  const sh = T.shares(T.stretchesOf(d));
  const colour = T.palette(sh.rows.map((r) => r.key));
  return sh.rows.map((r) => [
    r.key ? (sh.byProject ? projects.find((p) => p.id === r.key)?.name ?? r.key : r.key) : sh.byProject ? "Not sorted" : "Other",
    T.length(r.seconds),
    colour(r.key) < 0 ? "grey" : T.COLOURS[colour(r.key)],
  ]);
}

test("a_day_across_recordings", () => {
  let d = day("2026-10-19");
  expect(d.blocks.length).toBe(1);
  let a = d.blocks[0];
  expect(a.session).toBe("20261019-231500");
  expect(line(a)).toBe("23:15 – 23:59 · 45 min · Website");
  expect(kinds(a)).toEqual([
    "23:15 – 23:30 Google Chrome · Pricing — Linear · 1 read",
    "23:40 Figma · Pricing v2 · 1 read",
    "Not read 23:45 – 23:55: a private app was in front",
    "note 23:59",
  ]);
  const first = a.entries[0].stretch;
  expect([first.where, first.project, T.length(first.seconds)]).toEqual(["linear.app/acme/pricing", "web", "15 min"]);
  const read = first.reads[0];
  expect(read.preview).toBe("Pricing\nFree\nTeams $12");
  expect(read.more).toBe(true);
  expect(read.text).toBe("Pricing\nFree\nTeams $12\nEnterprise\nContact us");
  expect(a.entries[1].stretch.reads[0].lookedAt).toBe(true);
  const voice = a.entries[3].moment;
  expect(voice.more).toBe(false);
  expect(voice.preview).toBe("We keep annual at twenty percent off because churn drops.");
  expect(T.noteCaption(voice, D.time(voice.t))).toBe("You said · 23:59 · in Figma");
  expect(a.project).toBe("web");

  d = day("2026-10-20");
  expect(d.blocks.map((b) => b.session)).toEqual(["20261019-231500", "20261020-093000", "20261020-140000"]);
  const [a2, b, c] = d.blocks;
  expect(kinds(a2)).toEqual([
    "00:05 – 00:20 Code · main.rs — capsule · 1 read",
    "Not read 00:10 – 00:14: the screen was locked",
    "Not read 00:16 – 00:17: not read",
  ]);
  expect(line(a2)).toBe("00:05 – 00:20 · 15 min · Code");
  expect(line(b)).toBe("09:30 – 10:10 · 40 min · Mobile app, Client report");
  expect(kinds(b)).toEqual([
    "09:30 – 10:00 Slack · #design — Acme · 1 read",
    "Not read 10:00: this window wasn't shared",
    "note 10:01",
    "10:02 – 10:10 Google Chrome · Acme report · 1 read",
  ]);
  expect(b.entries[3].stretch.project).toBe("report");
  const note = b.entries[2].moment;
  expect(T.noteCaption(note, D.time(note.t))).toBe("You typed · 10:01");
  expect(note.project).toBe("app");
  expect(c.recording && !a2.recording).toBe(true);
  expect(line(c)).toBe("14:00 – 14:29 · 29 min · Code");

  expect(T.recorded(d.blocks)).toBe(900 + 1797 + 480 + 1737);
  expect(T.summary(d)).toBe("1 h 22 min recorded · 3 recordings");
  expect(went(d)).toEqual([
    ["Not sorted", "44 min", "grey"],
    ["Mobile app", "30 min", "blue"],
    ["Client report", "8 min", "teal"],
  ]);

  // The day bar: from the first stretch's hour to the end of the last one's; about 8 labels.
  const range = T.barRange(T.stretchesOf(d));
  expect([D.time(range.from), D.time(range.to)]).toEqual(["00:00", "15:00"]);
  const ticks = T.barTicks(range);
  expect(ticks.map((t) => D.hour(t.t))).toEqual(["00", "02", "04", "06", "08", "10", "12", "14"]);
  expect(ticks[0].at).toBe(0);
  const slack = T.stretchesOf(d)[1];
  expect(slack.app).toBe("Slack");
  expect(Math.abs((slack.start - range.from) / (range.to - range.from) - (9.5 * 3600 + 3) / (15 * 3600))).toBeLessThan(1e-9);
  const colour = T.palette(T.shares(T.stretchesOf(d)).rows.map((r) => r.key));
  expect(colour(slack.project)).toBe(0);
  expect(colour(T.stretchesOf(d)[0].project)).toBe(-1);

  expect(day("2026-10-18").blocks.length).toBe(0);
  expect(T.summary(day("2026-10-18"))).toBe("Nothing recorded");
  expect(T.barRange([])).toBeNull();
  expect(day("2026-10-23").blocks.length).toBe(1);
});

// MARK: stretches

const ID = "20261021-090000";
const BASE = Date.parse("2026-10-21T07:00:00Z"); // 09:00 local
const at = (s) => new Date(BASE + s * 1000).toISOString().replace(".000Z", "Z");
const read = (s, app, title, more = {}) => JSON.stringify({ t: at(s), end: at(s + 3), app, bundle: "", title, src: "ax", text: "", ...more });
const plain = (s, app, title) => read(s, app, title);
const one = (events, gaps = [], assign) => ({ [ID]: { events: lines(...events), gaps: lines(...gaps), assign } });
const blockOf = (files, projects = PROJECTS) => T.loadDay("2026-10-21", files, { projects }).blocks[0];
const shapes = (b) => b.entries.map((e) => (e.type === "stretch" ? `${e.stretch.name} ${e.stretch.reads.length}` : e.type));

test("same_app_and_title_in_a_row_is_one_stretch", () => {
  const b = blockOf(one([plain(0, "Chrome", "Pricing"), plain(3, "Chrome", "Pricing"), plain(6, "Chrome", "Pricing"), plain(9, "Chrome", "Docs"), plain(12, "Code", "Docs"), plain(15, "Chrome", "Pricing")]));
  expect(shapes(b)).toEqual(["Chrome · Pricing 3", "Chrome · Docs 1", "Code · Docs 1", "Chrome · Pricing 1"]);
  const s = b.entries[0].stretch;
  expect([span(s.start, s.end), T.length(s.seconds), T.readsLine(s)]).toEqual(["09:00", "under 1 min", "3 reads"]);
  expect(s.seconds).toBe(9);
  expect(T.readsLine(b.entries[1].stretch)).toBe("1 read");
  expect(s.id).toBe(`${ID}#0`);
  expect(b.entries[3].stretch.id).toBe(`${ID}#5`);
  expect(b.entries[0].id).toBe(`s-${ID}#0`);
});

test("an_empty_title_matches_only_an_empty_title", () => {
  const b = blockOf(one([plain(0, "Finder", ""), plain(3, "Finder", ""), plain(6, "Finder", "Downloads"), plain(9, "Finder", ""), plain(12, "", ""), plain(15, "", "")]));
  expect(shapes(b)).toEqual(["Finder 2", "Finder · Downloads 1", "Finder 1", "A window 2"]);
});

test("a_note_a_gap_or_a_pause_ends_a_stretch", () => {
  const note = JSON.stringify({ t: at(6), end: at(8), kind: "voice", src: "voice", app: "Chrome", title: "Pricing", text: "Annual stays." });
  const b = blockOf(
    one(
      [plain(0, "Chrome", "Pricing"), plain(3, "Chrome", "Pricing"), note, plain(9, "Chrome", "Pricing"), plain(30, "Chrome", "Pricing"), plain(60, "Chrome", "Pricing"), plain(123, "Chrome", "Pricing"), plain(187, "Chrome", "Pricing")],
      [JSON.stringify({ t: at(40), end: at(50), why: "private_app" })],
    ),
  );
  expect(shapes(b)).toEqual(["Chrome · Pricing 2", "note", "Chrome · Pricing 2", "gap", "Chrome · Pricing 2", "Chrome · Pricing 1"]);
  const g = b.entries[3].gap;
  expect(T.gapLine(g, span(g.t, g.end))).toBe("Not read 09:00: a private app was in front");
});

test("what_a_stretch_shows", () => {
  const long = "x".repeat(130);
  const b = blockOf(
    one([
      read(0, "Chrome", "Pricing", { text: "short\nAlso short line here!", project: "web" }),
      read(3, "Chrome", "Pricing", { where: "linear.app/pricing", text: `Menu\n   Pricing for teams of any size   \n${long}`, project: "app" }),
      read(6, "Chrome", "Pricing", { where: "linear.app/other", text: "tiny", project: "app" }),
      read(9, "Chrome", "Pricing", { text: "", project: "web" }),
      read(12, "Chrome", "Pricing", { project: "web" }),
      read(100, "Code", "", { text: "fn main() {}\nlet x = 1;" }),
      read(200, "Mail", "Inbox", { text: long }),
    ]),
  );
  const s = b.entries[0].stretch;
  expect(s.where).toBe("linear.app/pricing");
  expect(s.peek).toBe("Pricing for teams of any size");
  expect(s.project).toBe("web");
  expect(s.reads.length).toBe(5);
  expect(b.entries[1].stretch.peek).toBe("");
  expect(b.entries[1].stretch.project).toBe("");
  const cut = b.entries[2].stretch.peek;
  expect([Array.from(cut).length, cut.endsWith("x…")]).toEqual([121, true]);
  // A tie: the project seen first.
  const tie = blockOf(one([read(0, "A", "", { project: "app" }), read(3, "A", "", { project: "web" })]));
  expect(tie.entries[0].stretch.project).toBe("app");
  // Byte counts as in Swift's utf8.count.
  expect(blockOf(one([read(0, "A", "", { text: "é€😀" })])).entries[0].stretch.reads[0].size).toBe(2 + 3 + 4);
});

test("the_project_counts_corrections", () => {
  const b = blockOf(
    one([read(0, "Chrome", "Pricing", { project: "web" }), read(3, "Chrome", "Pricing", { project: "web" }), read(6, "Chrome", "Pricing", { project: "web" })], [], JSON.stringify({ assign: [{ from: at(3), to: at(100), project: "app" }] })),
  );
  expect(b.entries[0].stretch.project).toBe("app");
  expect(b.project).toBe("app");
});

test("recorded_time_and_where_it_went", () => {
  const events = [];
  for (let s = 0; s < 1800; s += 3) events.push(read(s, "Chrome", "Pricing", { project: "web" }));
  for (let s = 1800; s < 2400; s += 3) events.push(plain(s, "Code", "main.rs"));
  for (let s = 3600; s < 4800; s += 3) events.push(read(s, "Chrome", "Docs", { project: "web" }));
  let d = T.loadDay("2026-10-21", one(events), { projects: PROJECTS });
  expect(T.recorded(d.blocks)).toBe(1800 + 600 + 1200);
  expect(T.summary(d)).toBe("1 h recorded · 1 recording");
  expect(went(d).map((r) => r.slice(0, 2))).toEqual([["Website", "50 min"], ["Not sorted", "10 min"]]);
  const rows = T.shares(T.stretchesOf(d)).rows;
  expect(rows[1].seconds / rows[0].seconds).toBe(0.2);
  expect(line(d.blocks[0])).toBe("09:00 – 10:20 · 1 h 20 min · Website");
  const range = T.barRange(T.stretchesOf(d));
  expect(T.barTicks(range).map((t) => D.hour(t.t))).toEqual(["09", "10", "11"]);
  const colour = T.palette(rows.map((r) => r.key));
  expect(T.stretchesOf(d).map((s) => colour(s.project))).toEqual([0, -1, 0]);

  // No project anywhere: per app; the recording names its top 2 apps.
  const ev2 = [];
  ["Chrome", "Code", "Slack", "Chrome"].forEach((app, i) => {
    for (let s = i * 600; s < i * 600 + (app === "Slack" ? 60 : 300); s += 3) ev2.push(plain(s, app, "x"));
  });
  d = T.loadDay("2026-10-21", one(ev2), { projects: [] });
  const sh = T.shares(T.stretchesOf(d));
  expect(sh.byProject).toBe(false);
  expect(went(d, []).map((r) => [r[0], r[2]])).toEqual([["Chrome", "blue"], ["Code", "teal"], ["Slack", "orange"]]);
  expect(T.names(d.blocks[0]).keys).toEqual(["Chrome", "Code"]);
  const c2 = T.palette(sh.rows.map((r) => r.key));
  expect(T.stretchesOf(d).map((s) => c2(T.paletteKey(s, sh.byProject)))).toEqual([0, 1, 2, 0]);
  // A read with no app, per app: "Other", grey.
  d = T.loadDay("2026-10-21", one([plain(0, "", "x"), plain(3, "", "x"), plain(100, "Code", "y")]), { projects: [] });
  expect(went(d, []).map((r) => [r[0], r[2]])).toEqual([["Other", "grey"], ["Code", "blue"]]);
});

test("lengths", () => {
  expect(T.length(59)).toBe("under 1 min");
  expect(T.length(60)).toBe("1 min");
  expect(T.length(38 * 60 + 29)).toBe("38 min");
  expect(T.length(38 * 60 + 30)).toBe("39 min");
  expect(T.length(59 * 60 + 40)).toBe("1 h");
  expect(T.length(3600)).toBe("1 h");
  expect(T.length(110 * 60)).toBe("1 h 50 min");
  expect(T.length(-5)).toBe("under 1 min");
  expect(T.readsLine({ reads: [1], found: null })).toBe("1 read");
  expect(T.readsLine({ reads: Array(17), found: [1, 2, 3] })).toBe("3 of 17 reads match");
  expect(new Set(T.COLOURS).size).toBe(6);
  expect(T.COLOURS).toEqual(["blue", "teal", "orange", "green", "pink", "brown"]);
});

test("search_the_day", () => {
  const d = day("2026-10-20");
  const found = { blocks: T.filterDay(d, "ACME") };
  expect(T.stretchesOf(found).map((s) => s.name)).toEqual(["Slack · #design — Acme", "Google Chrome · Acme report"]);
  expect(found.blocks.map((b) => b.entries.length)).toEqual([0, 2, 0]);
  expect(found.blocks.every((b) => b.entries.every((e) => e.type !== "gap"))).toBe(true);
  const hits = (q) => T.filterDay(d, q).map((b) => b.entries.length);
  expect(hits("docs.google ship")).toEqual([0, 0, 0]);
  expect(hits("slack friday")).toEqual([0, 1, 0]);
  expect(hits("capsule\\main")).toEqual([1, 0, 0]);
  expect(hits("ACMÉ")).toEqual([0, 2, 0]);
  expect(T.searchable("Éclair ＦＵＬＬ")).toBe("eclair full");
  expect(T.filterDay(d, "  ")).toBe(d.blocks);
  const notes = T.filterDay(d, "without annual");
  expect(notes[1].entries.map((e) => e.type)).toEqual(["note"]);

  const events = ["one apple", "two pears", "three apples"].map((t, i) => read(i * 3, "Notes", "Fruit", { text: t }));
  const d2 = T.loadDay("2026-10-21", one(events), { projects: [] });
  const s = T.stretchesOf({ blocks: T.filterDay(d2, "apple") })[0];
  expect([s.reads.length, s.found.length, s.id]).toEqual([3, 2, `${ID}#0`]);
  expect(s.found.map((m) => m.text)).toEqual(["one apple", "three apples"]);
  expect(T.readsLine(s)).toBe("2 of 3 reads match");
  expect(T.readsLine(T.stretchesOf(d2)[0])).toBe("3 reads");
});

test("the delete words", () => {
  const d = day("2026-10-19");
  const looked = d.blocks[0].entries[1].stretch.reads[0];
  expect(looked.line).toBe(A_EVENTS.split("\n")[1]);
  expect(looked.stamp).toBe("2026-10-19T21:40:00Z");
  expect(T.deleteMomentMenu(looked)).toBe("Delete this read…");
  expect(T.deleteMomentText(looked)).toEqual({ title: "Delete this read? It can't be undone.", message: "It's removed from this Mac. Capsules already made stay as they are." });
  const voice = d.blocks[0].entries[3].moment;
  expect(T.deleteMomentMenu(voice)).toBe("Delete this voice note…");
  expect(T.deleteMomentText(voice, "this computer")).toEqual({ title: "Delete this voice note? It can't be undone.", message: "It's removed from this computer. Capsules already made stay as they are." });
  const typed = day("2026-10-20").blocks[1].entries[2].moment;
  expect(T.deleteMomentText(typed).title).toBe("Delete this note? It can't be undone.");

  const same = plain(3, "Chrome", "Pricing");
  const b = blockOf(one([plain(0, "Chrome", "Pricing"), same, same, plain(9, "Code", "x")]));
  const s = b.entries[0].stretch;
  expect(s.reads.length).toBe(3);
  expect(T.deleteStretchText(s, span(s.start, s.end), T.thisComputerFor("windows"))).toEqual({
    title: "Delete this stretch? It can't be undone.",
    message: "All 3 reads from 09:00 are removed from this PC. Capsules already made stay as they are.",
  });
  expect(T.deleteStretchText({ reads: [1] }, "10:00 – 10:05").message).toBe("Its read from 10:00 – 10:05 is removed from this Mac. Capsules already made stay as they are.");
  expect(T.deleteBlockText(b, "09:00", "09:12")).toEqual({
    title: "Delete this recording? It can't be undone.",
    message: "Everything recorded from 09:00 to 09:12 is removed from this Mac. Capsules already made stay as they are.",
  });
  expect(T.refusedText("recording")).toEqual({ title: "Stop first", message: "This recording is still going. Stop it, then delete." });
  expect(T.refusedText("gone")).toEqual({ title: "Already gone", message: "It isn't there anymore. The page is up to date now." });
  expect(T.refusedText("Disk full")).toEqual({ title: "Couldn't delete", message: "Disk full" });
  expect([T.thisComputerFor("mac"), T.thisComputerFor("linux")]).toEqual(["this Mac", "this computer"]);
});

test("words_and_folding", () => {
  expect(T.sessionPrompt("20261020-093000")).toBe("Use Context Capsule to make a capsule from my session 20261020-093000.");
  expect(T.fold("a\nb\nc")).toEqual(["a\nb\nc", false]);
  expect(T.fold("a\nb\nc\n")).toEqual(["a\nb\nc", false]);
  expect(T.fold("a\r\nb\r\nc\r\nd")).toEqual(["a\nb\nc", true]);
  expect(T.fold("  a  \n\n\t\n b")).toEqual(["a\nb", false]);
  const [text, more] = T.fold("x".repeat(500));
  expect(more && Array.from(text).length === 300 && text.endsWith("…")).toBe(true);
  expect(T.peek("short\n  " + "y".repeat(20) + "  ")).toBe("y".repeat(20));
  expect(T.emptyLine({ today: true, recordKey: "Alt+C" })).toBe("Nothing recorded today yet. Start contextualizing (Alt+C) when you begin work.");
  expect(T.emptyLine({ recordKey: "Alt+C" })).toBe("Nothing recorded on this day.");
  expect(T.emptyLine({ searching: true, today: true })).toBe("Nothing on this day matches.");
  expect(T.isSessionID("20261019-231500") && !T.isSessionID("2026101-9231500") && !T.isSessionID("../../etc/x")).toBe(true);
  for (const w of ["private_app", "not_shared", "no_text", "system", "locked"]) expect(T.gapMessage(w)).not.toBe("not read");
  expect(T.mostUsed(["", "a", "b", "b", "a"])).toBe("b"); // b got ahead first
  expect(T.mostUsed(["a", "b"])).toBe("a");
  expect(T.mostUsed(["", ""])).toBe("");
});

test("choosing_the_day", () => {
  expect(T.candidates(Object.keys(FILES), "2026-10-20")).toEqual(["20261019-231500", "20261020-093000", "20261020-140000"]);
  expect(T.candidates(["20261231-230000", "20270101-010000", "x"], "2027-01-01")).toEqual(["20261231-230000", "20270101-010000"]);
  expect([...T.recordedDays(Object.keys(FILES))].sort()).toEqual(["2026-10-19", "2026-10-20", "2026-10-23"]);
  expect(new Date(T.sessionStart("20261019-231500")).toISOString()).toBe("2026-10-19T21:15:00.000Z");
  expect(T.sessionStart("20261340-000000")).toBeNull();
  expect(addDays("2026-01-01", -1)).toBe("2025-12-31");
  // A session that starts that day but has nothing yet: shown, from its start; its end from session.json.
  const d = T.loadDay("2026-10-21", { [ID]: { session: `{"ended_at":"2026-10-21T07:05:00Z"}` } }, {});
  expect(d.blocks.length).toBe(1);
  expect([D.time(d.blocks[0].start), D.time(d.blocks[0].end)]).toEqual(["09:00", "09:05"]);
  expect(T.summary(d)).toBe("1 recording");
});
