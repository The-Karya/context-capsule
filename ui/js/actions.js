// context-capsule — ui: what the buttons do with Your AI (SPEC "Any AI"): open it with a request, Copy my day,
// Save from clipboard, connect apps, open a received capsule; and sharing a capsule (Check before sharing)

import { call, why, read } from "./host.js";
import { alert, toast, h } from "./dom.js";
import * as AI from "./core/yourai.js";
import * as Caps from "./core/capsules.js";
import * as Check from "./core/sharecheck.js";
import { sessionPrompt } from "./core/timeline.js";

const changed = (app) => app.refresh("ai");

async function mcp(command, args = {}) {
  const out = await call("mcp.run", { command, ...args });
  return typeof out === "string" ? out : JSON.stringify(out);
}

/** Your AI gets a request: Claude (connected first), another connected app, or Claude for websites / any AI. */
export async function open(app, prompt, whyText, again) {
  const kind = app.kind();
  const c = kind.type === "app" ? app.clients.find((x) => x.id === kind.id) : null;
  if (c) {
    if (!c.connected) {
      const t = AI.connectFirst(c.name, whyText, again);
      const i = await alert({ title: t.title, message: t.message, buttons: [{ text: "Connect", kind: "default" }, { text: "Cancel", kind: "cancel" }] });
      if (i === 0) await connect(app, c);
      return;
    }
    const r = await call("ai.open", { id: c.id, prompt }).catch(() => ({ opened: false }));
    toast(AI.copiedFor(c.name, !!r?.opened));
    return;
  }
  // Claude: without the connection Claude can't see the capsules, so connect first instead of an empty chat.
  await call("clipboard.write", { text: prompt }).catch(() => {});
  const claude = app.clients.find((x) => x.id === AI.claudeID);
  if (claude && !claude.installed) {
    await call("setup.do", { id: "ai" }).catch(() => {}); // says how to get the Claude app
    return;
  }
  if (claude && !claude.connected) {
    const i = await alert({
      title: "Connect Claude first",
      message: `Claude needs Context Capsule installed ${whyText}. Click Connect, then Install in Claude. After that, ${again}.`,
      buttons: [{ text: "Connect", kind: "default" }, { text: "Cancel", kind: "cancel" }],
    });
    if (i === 0) await call("setup.do", { id: "ai" }).catch(() => {});
    return;
  }
  await call("ai.open", { id: AI.claudeID, prompt }).catch((e) => alert({ title: "Couldn't open Claude", message: why(e) }));
}

/** Make today's capsules, the way Your AI works. */
export function makeTodays(app) {
  const kind = app.kind();
  if (AI.usesCopy(kind)) return copyDay(app, null);
  return open(app, Caps.dayPrompt, "to read your day", "click Make today's capsules again");
}

/** Timeline → Make a capsule: one recording (websites and any AI get that day's Copy my day). */
export function makeCapsule(app, session, date) {
  if (AI.usesCopy(app.kind())) return copyDay(app, date);
  return open(app, sessionPrompt(session), "to read your recording", "click Make a capsule from this again");
}

/** Ask your capsules (the Capsules page). */
export function ask(app, question) {
  if (!question.trim()) return;
  return open(app, Caps.searchPrompt(question), "to search your capsules", "click Ask again");
}

/** Copy my day: the day as one message on the clipboard; the website opens; then Save from clipboard. */
export async function copyDay(app, date) {
  if (app.ai.working) return;
  app.ai.working = true;
  changed(app);
  try {
    const day = AI.parseDay(await mcp("day", date ? { date } : {}));
    if (!day.text.trim()) {
      toast(date ? "Nothing recorded that day." : AI.words.nothingToday);
      return;
    }
    await call("clipboard.write", { text: day.text });
    const kind = app.kind();
    if (kind.type === "website") call("open.url", { url: kind.url }).catch(() => {});
    app.ai.dayLine = AI.pasteLine;
    toast(AI.pasteLine);
  } catch (e) {
    await alert({ title: AI.words.copyFailed, message: AI.failureMessage(why(e)) });
  } finally {
    app.ai.working = false;
    changed(app);
  }
}

/** Save from clipboard: the capsules in the AI's reply; then what was saved and what wasn't (and why). */
export async function saveFromClipboard(app) {
  if (app.ai.working) return;
  const text = (await call("clipboard.read").catch(() => ({ text: "" })))?.text || "";
  if (!text.trim()) {
    await alert({ title: AI.words.clipboardEmpty, message: AI.words.clipboardEmptyBody });
    return;
  }
  app.ai.working = true;
  changed(app);
  try {
    const result = AI.parseSave(await mcp("save", { input: text }));
    if (result.saved.length) {
      app.ai.dayLine = null;
      await app.loadCapsules();
      await app.loadStats();
      app.refresh("capsules");
    }
    const s = AI.saveSummary(result);
    await alert({ title: s.title, message: s.detail });
  } catch (e) {
    await alert({ title: AI.words.saveFailed, message: AI.failureMessage(why(e)) });
  } finally {
    app.ai.working = false;
    changed(app);
  }
}

/** Settings → Connect your AI → Connect. Claude installs its extension; others are written into their settings. */
export async function connect(app, c) {
  if (c.how === "extension" || c.id === AI.claudeID) {
    await call("setup.do", { id: "ai" }).catch(() => {});
    app.ai.note = AI.words.claudeInstallNote;
    changed(app);
    return;
  }
  const before = app.clients;
  app.ai.busy = c.id;
  app.ai.note = null;
  changed(app);
  try {
    const a = AI.parseConnect(await mcp("connect", { id: c.id }));
    if (a.ok) {
      app.ai.note = a.message || AI.connectedNote(c.name);
      if (AI.firstConnectedBecomesChoice(app.config.ai, before)) await setChoice(app, c.id);
    } else {
      await alert({ title: AI.connectFailed(c.name), message: a.message || "Please try again." });
    }
  } catch (e) {
    await alert({ title: AI.connectFailed(c.name), message: AI.failureMessage(why(e)) });
  }
  app.ai.busy = null;
  await app.loadClients();
  changed(app);
}

export async function disconnect(app, c) {
  app.ai.busy = c.id;
  app.ai.note = null;
  changed(app);
  try {
    const a = AI.parseConnect(await mcp("disconnect", { id: c.id }));
    if (a.ok) app.ai.note = a.message || AI.disconnectedNote(c.name);
    else if (a.how === "extension") await alert({ title: AI.words.removeInClaude, message: AI.words.removeInClaudeBody });
    else await alert({ title: AI.disconnectFailed(c.name), message: a.message || "Please try again." });
  } catch (e) {
    await alert({ title: AI.disconnectFailed(c.name), message: AI.failureMessage(why(e)) });
  }
  app.ai.busy = null;
  await app.loadClients();
  changed(app);
}

export async function setChoice(app, id) {
  app.config = { ...app.config, ai: id };
  app.ai.dayLine = null;
  await call("config.set", { key: "ai", value: id }).catch(() => {});
  changed(app);
}

/** A capsule someone sent (picked, dropped or opened with the app): check it, then your AI carries on from it. */
export async function continueFrom(app, path, text) {
  if (!Caps.isCapsule(text)) {
    const name = path.split(/[\\/]/).pop();
    await alert({ title: "This isn't a capsule", message: `${name} isn't a Context Capsule file.` });
    return;
  }
  return open(app, Caps.continuePrompt(path), "to read the capsule", "open the capsule again");
}

/** "Open a capsule…". */
export async function pickCapsule(app) {
  const f = await call("capsule.pick").catch(() => null);
  if (f) await continueFrom(app, f.path, f.text);
}

// ---------- sharing ----------

/** Check before sharing: the cleaned text to share, undefined to share the file as it is, or null when cancelled. */
async function checked(file) {
  const text = (await read([file]))[file];
  if (text == null) return undefined;
  const groups = Check.nonEmpty(Check.find(text));
  if (!groups.length) return undefined;
  const boxes = [];
  const list = h("div", { class: "checks", role: "group", "aria-label": "Items that might be private" });
  for (const g of groups) {
    list.append(h("h3", null, g.title));
    for (const item of g.items) {
      const box = h("input", { type: "checkbox", checked: true });
      boxes.push([box, item]);
      list.append(h("label", { title: item }, box, item));
    }
  }
  const i = await alert({
    title: "Check before sharing",
    message: "Checked items become [removed] in the copy you share. Your saved capsule stays the same.",
    extra: list,
    wide: true,
    buttons: [{ text: "Cancel", kind: "cancel" }, { text: "Share", kind: "default" }],
  });
  if (i !== 1) return null;
  const remove = boxes.filter(([b]) => b.checked).map(([, item]) => item);
  return remove.length ? Check.remove(text, remove) : undefined;
}

const SHARED = {
  "copy-file": "Copied. Paste it in WhatsApp, Slack, email or any chat.",
  "copy-text": "Text copied. Paste it into a chat, or into your AI.",
};

export async function share(app, c, how) {
  if (how === "reveal") return call("capsule.share", { file: c.file, how }).catch(() => {});
  const text = await checked(c.file);
  if (text === null) return;
  try {
    const r = await call("capsule.share", { file: c.file, how, ...(text !== undefined ? { text } : {}) });
    if (how === "save-copy" && r?.done) toast(`Saved to ${r.where || "the folder you chose"}.`);
    else if (SHARED[how]) toast(SHARED[how]);
  } catch (e) {
    toast(`Couldn't share: ${why(e)}`);
  }
}

export function openCapsule(c) {
  call("capsule.open", { file: c.file }).catch(() => {});
}
