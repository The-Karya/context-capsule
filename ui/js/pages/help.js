// context-capsule — ui: Help — the shortcuts, how it works, and privacy, in plain words

import { h } from "../dom.js";
import * as AI from "../core/yourai.js";

export function mount(root, app) {
  const draw = () => root.replaceChildren(page(app));
  draw();
  return { refresh: draw };
}

function page(app) {
  const keys = app.keys();
  const computer = app.platform === "mac" ? "this Mac" : app.platform === "windows" ? "this PC" : "this computer";
  const line = (k, text, key = true) => h("div", { class: "row h40", role: "listitem" }, h("span", { class: "label" }, text), key ? h("kbd", { class: "keycap" }, k) : h("span", { class: "keys" }, k));
  const text = (lines) => h("div", { class: "help-text" }, lines.map((l) => h("p", null, l)));
  return h(
    "div",
    { class: "content" },
    h("div", { class: "page-head" }, h("h1", { class: "title" }, "Help")),
    h("h2", { class: "caption", id: "h-k", style: { marginTop: "4px" } }, "Shortcuts"),
    h(
      "div",
      { class: "rows", role: "list", "aria-labelledby": "h-k" },
      line(keys.record, "Start or stop contextualizing"),
      line(keys.voice, "Speak a note, then again when done"),
      line(AI.helpMakeKeys(app.kind()), "Make my capsules for today", false),
    ),
    h("h2", { class: "caption" }, "How it works"),
    text([
      "While you work, Context Capsule saves the text of the screen you picked, and what you say in voice notes.",
      "At the end of the day, your AI sorts it into projects and writes a capsule for each: what you did, what you decided and why, and what's next.",
      "With Claude Code, Codex or Gemini CLI on this computer, that can happen by itself at a time you pick (Settings → Make capsules).",
      "Any AI works. Apps like Claude or Cursor connect in Settings. For any other AI, such as a website: Copy my day, paste it in, then Save from clipboard.",
      "Send a capsule to anyone. Their AI reads it and can carry on your work.",
    ]),
    h("h2", { class: "caption" }, "Privacy"),
    text([
      `Recordings stay on ${computer}. No account, no cloud.`,
      "Only text is saved: no screenshots, no audio.",
      "Password managers, WhatsApp, Signal and password fields are never recorded.",
      "Text goes to your AI only when it makes or reads capsules (by itself only if you chose Automatically), or when you paste your day into it.",
    ]),
  );
}
