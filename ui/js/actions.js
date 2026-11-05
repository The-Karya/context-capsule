// context-capsule — ui: what the buttons do with Your AI (SPEC "Any AI"): open it with a request, Copy my day,
// Save from clipboard, connect apps, open a received capsule; and sharing a capsule (the share page)

import { call, why, read } from "./host.js";
import { alert, toast } from "./dom.js";
import * as AI from "./core/yourai.js";
import * as Auto from "./core/auto.js";
import * as Caps from "./core/capsules.js";
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

/** Make today's capsules: in the background with an AI tool on this computer when there is one (SPEC "Automatic
 * capsules"), else the way Your AI works. */
export async function makeTodays(app) {
  if (app.auto?.running) return;
  if (app.auto?.agent) {
    try {
      await call("auto.run");
      app.auto = { ...app.auto, running: true };
      app.refresh("auto");
      return;
    } catch (e) {
      const w = why(e);
      if (w === "running") return;
      if (w === "nothing") return toast(Auto.words.nothingToday);
      if (w !== "none") return alert({ title: "Couldn't make today's capsules", message: AI.failureMessage(w) });
      // no tool after all: Your AI, as before
    }
  }
  return makeWithYourAI(app);
}

/** Make today's capsules with Your AI: open it with the request, or Copy my day. */
export function makeWithYourAI(app) {
  const kind = app.kind();
  if (AI.usesCopy(kind)) return copyDay(app, null);
  return open(app, Caps.dayPrompt, "to read your day", "click Make today's capsules again");
}

/** Timeline → Make a capsule: one recording (websites and any AI get that day's Copy my day). */
export function makeCapsule(app, session, date) {
  if (AI.usesCopy(app.kind())) return copyDay(app, date);
  return open(app, sessionPrompt(session), "to read your recording", "click Make a capsule from this again");
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
    // when you come back with the AI's answer copied, the window offers to save it (offerAnswer)
    app.ai.awaiting = { since: Date.now(), copied: day.text, offered: "" };
    const kind = app.kind();
    if (kind.type === "website") call("open.url", { url: kind.url }).catch(() => {});
    // an app that makes capsules this way (the ChatGPT app): it opens, the day still on the clipboard
    if (kind.type === "app") call("ai.open", { id: kind.id, prompt: day.text }).catch(() => {});
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
      app.ai.awaiting = null;
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

/** Back in the window after Copy my day, with your AI's answer copied: offer to save it, once per answer. Only
 * looks at the clipboard in the hours after Copy my day, while the window is in front; saves only on Save. */
export async function offerAnswer(app) {
  const w = app.ai.awaiting;
  if (!w || app.ai.working || app.ai.offering) return;
  if (Date.now() - w.since > AI.ANSWER_WAIT_MS) {
    app.ai.awaiting = null;
    return;
  }
  app.ai.offering = true;
  try {
    const text = (await call("clipboard.read").catch(() => null))?.text || "";
    const n = AI.answerCount(text, w.copied);
    if (!n || text === w.offered) return;
    w.offered = text;
    const o = AI.answerOffer(n);
    const i = await alert({ title: o.title, message: o.message, buttons: [{ text: "Save", kind: "default" }, { text: "Not now", kind: "cancel" }] });
    if (i === 0) await saveFromClipboard(app);
  } finally {
    app.ai.offering = false;
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
  await app.loadAuto(); // the tool that makes capsules may follow Your AI
  changed(app);
}

/** A capsule from outside (opened with the app, dropped, or picked): checked, then shown in the reading view. */
export async function received(app, path, text) {
  if (!Caps.isCapsule(text)) {
    const name = path.split(/[\\/]/).pop();
    await alert({ title: "This isn't a capsule", message: `${name} isn't a Context Capsule file.` });
    return;
  }
  // the note a shared copy starts with is for people without Context Capsule
  app.read({ path, text: Caps.withoutShareNote(text) });
}

/** The reading view's "Continue in <AI>": your AI carries on from the capsule (an AI without the app gets it pasted). */
export async function continueCapsule(app, { name, path, text, saved }) {
  const kind = app.kind();
  if (AI.usesCopy(kind)) {
    await call("clipboard.write", { text: Caps.pastePrompt(text) }).catch(() => {});
    if (kind.type === "website") call("open.url", { url: kind.url }).catch(() => {});
    if (kind.type === "app") call("ai.open", { id: kind.id, prompt: Caps.pastePrompt(text) }).catch(() => {});
    toast(`Copied. Paste it into ${app.aiName()}.`);
    return;
  }
  const prompt = saved ? Caps.continueSavedPrompt(name) : Caps.continuePrompt(path);
  return open(app, prompt, "to read the capsule", "click Continue again");
}

/** "Open a capsule…". */
export async function pickCapsule(app) {
  const f = await call("capsule.pick").catch(() => null);
  if (f) await received(app, f.path, f.text);
}

/** A saved capsule's text (null if it's gone). */
export async function readCapsule(file) {
  return (await read([file]).catch(() => ({})))[file] ?? null;
}

// ---------- sharing ----------

const SHARED = {
  "copy-file": "Copied. Paste it in WhatsApp, Slack, email or any chat. It says how to use it, even without Context Capsule.",
  "copy-text": "Text copied. Paste it into a chat, or into your AI.",
};

/** Share… anywhere: the share page (the capsule as it will be shared; SPEC "Sharing a capsule"). */
export function openShare(app, c) {
  app.go("share", { file: c.file });
}

/** Share a capsule: `text` is the copy from the share page (with its note on top; undefined: the file as it is). */
export async function share(app, c, how, text) {
  if (how === "reveal") return call("capsule.share", { file: c.file, how }).catch(() => {});
  try {
    const r = await call("capsule.share", { file: c.file, how, ...(text !== undefined ? { text } : {}) });
    if (how === "save-copy" && r?.done) toast(`Saved to ${r.where || "the folder you chose"}.`);
    else if (SHARED[how]) toast(SHARED[how]);
  } catch (e) {
    toast(`Couldn't share: ${why(e)}`);
  }
}

/** A saved capsule's menu items: Share… (the share page) and Show in Finder. */
export function shareItems(app, c) {
  return [
    { text: "Share…", action: () => openShare(app, c) },
    { text: Caps.showLabel(app.platform), action: () => share(app, c, "reveal") },
  ];
}
