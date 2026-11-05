// context-capsule — ui: any AI (SPEC.md, "Any AI"): which AI is "Your AI", reading context-capsule-mcp's answers, and the words around them.

// A client (from `context-capsule-mcp clients`) is { id, name, installed, connected, how }.

export const anyID = "any";
export const claudeID = "claude";
export const websites = [
  { id: "chatgpt-web", name: "ChatGPT (website)", url: "https://chatgpt.com" },
  { id: "gemini-web", name: "Gemini (website)", url: "https://gemini.google.com" },
];

// Words.
export const pasteLine = "Paste it into your AI. When it answers, copy its whole reply and come back: Context Capsule offers to save it.";

/** How long after Copy my day the window looks for the AI's answer on the clipboard when you come back to it. */
export const ANSWER_WAIT_MS = 3 * 60 * 60 * 1000;

const START = /^[\s>*_`#-]*=+\s*CAPSULE\s*=+[\s*_`]*$/gim;

/** How many capsules a chat AI's answer on the clipboard holds (the "=== CAPSULE ===" lines, as `save` reads them).
 * 0: not an answer — empty, the day we copied ourselves (it shows the marker once, in its instructions), or text
 * that starts like it. */
export function answerCount(text, copied = "") {
  if (!text || !text.trim() || (copied && text.trim() === copied.trim()) || /^\s*I use Context Capsule\./.test(text)) return 0;
  return (text.match(START) || []).length;
}

/** The offer when you come back with the answer copied. */
export function answerOffer(n) {
  return {
    title: n === 1 ? "Your AI's capsule is on the clipboard" : `Your AI's ${n} capsules are on the clipboard`,
    message: n === 1 ? "Save it to Context Capsule?" : "Save them to Context Capsule?",
  };
}
export const words = {
  connectYourAI: "Connect your AI",
  connectHelp: "Lets your AI start contextualizing and write your capsules.",
  connect: "Connect",
  disconnect: "Disconnect",
  more: "More",
  notInstalled: "Not installed",
  anyOtherLine: "Any other AI (ChatGPT website, Gemini, Copilot…): use Copy my day.",
  claudeInstallNote: "Click Install in Claude. The ✓ shows when it's done.",
  yourAI: "Your AI",
  makeWith: "Make today's capsules with",
  anyOtherAI: "Any other AI",
  change: "Change",
  copyMyDay: "Copy my day",
  saveFromClipboard: "Save from clipboard",
  makeCapsules: "Make capsules",
  copyMyDayHelp: "Copies today's work as one message to paste into any AI.",
  saveHelp: "Saves the capsules in your AI's reply. Copy its whole reply first.",
  onTheWeb: "Using an AI on the web, or any other AI?",
  pasteLine,
  nothingToday: "Nothing recorded today yet.",
  copyFailed: "Couldn't copy your day",
  clipboardEmpty: "The clipboard is empty",
  clipboardEmptyBody: "Copy your AI's whole reply first, then click Save from clipboard.",
  saveFailed: "Couldn't save",
  removeInClaude: "Remove it in Claude",
  removeInClaudeBody: "In Claude, open Settings → Extensions and remove Context Capsule.",
};

// The id in use: the saved one, else Claude if it's connected, else the first connected app, else "any".
// A saved app id the program doesn't list (any more) falls back the same way; with no list yet it is kept.
export function resolve(saved, clients = []) {
  const s = typeof saved === "string" ? saved.trim() : "";
  if (s && (s === anyID || websites.some((w) => w.id === s) || clients.length === 0 || clients.some((c) => c.id === s))) {
    return s;
  }
  if (clients.some((c) => c.id === claudeID && c.connected)) return claudeID;
  return clients.find((c) => c.connected)?.id ?? anyID;
}

// An id -> { type: "claude" | "app" | "website" | "other", id, url? }.
export function kindOf(id) {
  id = (id || "").trim();
  if (id === claudeID) return { type: "claude", id };
  if (id === anyID || id === "") return { type: "other", id: anyID };
  const w = websites.find((x) => x.id === id);
  if (w) return { type: "website", id, url: w.url };
  return { type: "app", id };
}

// Apps that are connected but make capsules with Copy my day: the ChatGPT app loads Context Capsule's tools but
// doesn't give them to its chats yet (openai/codex#38162, open since Aug 2026), so a request would go nowhere.
export const copyApps = ["chatgpt"];

// Copy my day / Save from clipboard is the way (no MCP, or MCP that its chats can't use yet: copyApps).
export function usesCopy(kind) {
  return kind.type === "website" || kind.type === "other" || (kind.type === "app" && copyApps.includes(kind.id));
}

// Its name in sentences: "Claude", "Cursor", "ChatGPT", "your AI".
export function choiceName(kind, clients = []) {
  switch (kind.type) {
    case "claude": return "Claude";
    case "app": return clients.find((c) => c.id === kind.id)?.name ?? kind.id;
    case "website": return (websites.find((w) => w.id === kind.id)?.name ?? "your AI").replace(" (website)", "");
    default: return "your AI";
  }
}

// Settings → Connect your AI: installed apps first (Claude first among them), then by name.
export function sorted(clients) {
  return [...clients].sort((a, b) => {
    if (a.installed !== b.installed) return a.installed ? -1 : 1;
    if ((a.id === claudeID) !== (b.id === claudeID)) return a.id === claudeID ? -1 : 1;
    return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  });
}

// Settings → Your AI: the installed apps ("Cursor (not connected)" when not), the chosen one even if not
// installed, then the websites and "Any other AI". -> [{ id, label }]
export function choices(clients, current) {
  const out = clients
    .filter((c) => c.installed || c.id === current)
    .map((c) => ({ id: c.id, label: c.connected ? c.name : `${c.name} (not connected)` }));
  if (!clients.some((c) => c.id === current) && kindOf(current).type === "app") out.push({ id: current, label: current });
  for (const w of websites) out.push({ id: w.id, label: w.name });
  out.push({ id: anyID, label: words.anyOtherAI });
  return out;
}

// After connecting an app: it becomes Your AI if nothing was chosen and nothing else was connected yet.
export function firstConnectedBecomesChoice(saved, clientsBefore) {
  return (saved == null || saved === "") && !clientsBefore.some((c) => c.connected);
}

// Home: "Your AI: Cursor" / "Your AI: any AI".
export function homeLine(kind, name) {
  return `Your AI: ${kind.type === "other" ? "any AI" : name}`;
}

// Home → Make today's capsules, the line above the button(s). `enter`: "Return" on a Mac, "Enter" elsewhere.
// `dayLine`: shown instead once the day was copied (pasteLine).
export function makeLine(kind, name, dayLine = null, enter = "Return") {
  if (usesCopy(kind)) return dayLine ?? `Copy your day, paste it into ${name}, then save its reply here.`;
  if (kind.type === "claude") return `Claude opens with the request typed in. Press ${enter}.`;
  return `${name} opens. Paste the request in a new chat.`;
}

// Settings → Your AI, under the choice.
export function settingsLine(kind, name) {
  if (kind.type === "app" && copyApps.includes(kind.id)) {
    return `Make today's capsules copies your day and opens ${name}: paste it in, then you save its reply. (${name}'s chats can't use Context Capsule's tools yet.)`;
  }
  return usesCopy(kind)
    ? `Make today's capsules copies your day to paste into ${name}, then you save its reply.`
    : `Make today's capsules opens ${name} with the request.`;
}

// Help → Shortcuts, "Make my capsules for today".
export function helpMakeKeys(kind) {
  return usesCopy(kind) ? words.copyMyDay : "in your AI";
}

export function connectedNote(name) { return `Connected to ${name}. Restart ${name} to use it.`; }
export function disconnectedNote(name) { return `Disconnected from ${name}.`; }
export function connectFailed(name) { return `Couldn't connect ${name}`; }
export function disconnectFailed(name) { return `Couldn't disconnect ${name}`; }

// A request for an app that isn't connected yet. `why`: "to read your day"; `again`: "click Make today's capsules again".
export function connectFirst(name, why, again) {
  return { title: `Connect ${name} first`, message: `${name} needs Context Capsule ${why}. Click Connect. After that, ${again}.` };
}

// A request copied for another app: whether the app could be opened.
export function copiedFor(name, opened) {
  return opened ? `Copied. Paste it in a new chat in ${name}.` : `Copied. Open ${name} and paste it in a new chat.`;
}

// Why a call gave nothing usable: "missing" | "timeout" | "bad" | any other text (the program's own words).
export function failureMessage(why) {
  if (why === "missing") return "Part of Context Capsule is missing. Please download Context Capsule again.";
  if (why === "timeout") return "That took too long. Please try again.";
  if (typeof why === "string" && why !== "bad" && why.trim()) return why.trim();
  return "Something went wrong. Please try again.";
}

// MARK: the program's answers (its stdout text). Each throws an Error whose message is for failureMessage().

// The JSON value in the output. Tolerates lines before it: takes from the first line starting with { or [.
export function json(out) {
  try { return JSON.parse(out); } catch {}
  const lines = String(out).split("\n");
  const i = lines.findIndex((l) => l.startsWith("{") || l.startsWith("["));
  if (i >= 0) {
    try { return JSON.parse(lines.slice(i).join("\n")); } catch {}
  }
  throw new Error("bad");
}

const str = (o, k) => (o && typeof o[k] === "string" ? o[k] : null);
const nonEmpty = (s) => (s ? s : null);
const isObj = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
// {"error": "…"}: the program said why it couldn't.
const fail = (v) => new Error(isObj(v) && str(v, "error") !== null ? v.error : "bad");

export function parseClients(out) {
  const v = json(out);
  const list = Array.isArray(v) ? v : isObj(v) && Array.isArray(v.clients) ? v.clients : null;
  if (!list) throw new Error("bad");
  return list.flatMap((o) => {
    const id = nonEmpty(str(o, "id"));
    if (!id) return [];
    return [{
      id,
      name: nonEmpty(str(o, "name")) ?? id,
      installed: o.installed === true,
      connected: o.connected === true,
      how: str(o, "how") ?? "config",
    }];
  });
}

export function parseConnect(out) {
  const v = json(out);
  if (!isObj(v) || typeof v.ok !== "boolean") throw fail(v);
  return { ok: v.ok, message: str(v, "message") ?? "", how: str(v, "how") };
}

export function parseDay(out) {
  const v = json(out);
  const text = isObj(v) ? str(v, "text") : null;
  if (text === null) throw fail(v);
  return { text, chars: typeof v.chars === "number" ? v.chars : [...text].length };
}

export function parseSave(out) {
  const v = json(out);
  if (!isObj(v) || (!("saved" in v) && !("refused" in v))) throw fail(v);
  const list = (k) => (Array.isArray(v[k]) ? v[k] : []);
  const title = (o) => nonEmpty(str(o, "title")) ?? "Untitled";
  return {
    saved: list("saved").map((o) => ({ title: title(o), path: str(o, "path") ?? "" })),
    refused: list("refused").map((o) => ({ title: title(o), why: str(o, "why") ?? "" })),
  };
}

// What Save from clipboard says: a title, then what was saved and what wasn't (and why).
export const noCapsulesFound = "No capsules found in what you copied. Copy your AI's whole answer, then try again.";

// A refusal for "no capsule markers in the text": shown as plain words, never the "=== CAPSULE ===" lines.
const noMarkers = (r) => /^No capsule found|=== ?CAPSULE ?===/i.test(r.why ?? "");

export function saveSummary(result) {
  const saved = result.saved ?? [];
  const refused = (result.refused ?? []).filter((r) => !noMarkers(r));
  const n = saved.length;
  if (n === 0 && refused.length === 0) return { title: "Nothing was saved", detail: noCapsulesFound };
  const title = n === 0 ? "Nothing was saved" : `Saved ${n} capsule${n === 1 ? "" : "s"}`;
  const lines = saved.map((s) => `✓ ${s.title}`);
  if (refused.length) {
    if (lines.length) lines.push("");
    lines.push("Not saved:");
    for (const r of refused) lines.push(r.why ? `• ${r.title}: ${r.why}` : `• ${r.title}`);
  }
  return { title, detail: lines.join("\n") };
}
