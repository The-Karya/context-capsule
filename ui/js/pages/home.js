// context-capsule — ui: Home — the greeting, start/stop, today's numbers, time by project, recent capsules,
// shortcuts, what you're working on, and Make today's capsules

import { h, icon, button } from "../dom.js";
import { call } from "../host.js";
import * as Stats from "../core/stats.js";
import * as AI from "../core/yourai.js";
import { palette } from "../core/timeline.js";
import { capsuleRow, makeControl } from "./capsules.js";
import * as Auto from "../core/auto.js";
import * as actions from "../actions.js";

export function mount(root, app) {
  const draw = () => {
    const keep = root.scrollTop;
    root.replaceChildren(page(app));
    root.scrollTop = keep;
  };
  draw();
  return {
    refresh: draw,
    tick() {
      const t = root.querySelector("[data-live]");
      if (t) t.textContent = "Contextualizing  " + app.clock(app.elapsed());
    },
  };
}

/** One project's share of the time: name, a bar, the duration. */
export function timeBar(name, seconds, most, colour, muted, durText) {
  const w = most > 0 ? (seconds / most) * 100 : 0;
  return h(
    "div",
    { class: "bar-row", role: "listitem", "aria-label": `${name}: ${durText}` },
    h("span", { class: "name" + (muted ? " none" : ""), "aria-hidden": "true" }, name),
    h("span", { class: "track", "aria-hidden": "true" }, h("span", { class: "fill " + (colour < 0 ? "cn" : "c" + colour), style: { width: w + "%" } })),
    h("span", { class: "dur", "aria-hidden": "true" }, durText),
  );
}

function page(app) {
  const s = app.state;
  const now = app.now();
  const name = app.firstName();
  const hello = Stats.greeting(new Date(now).getHours());
  const content = h("div", { class: "content home" });

  content.append(
    h("div", { class: "greeting" }, h("h1", null, (name ? `${hello}, ${name}` : hello) + "."), h("p", null, app.dates.long(now, now))),
  );

  // Status card.
  const keys = app.keys();
  const startText = s.recording ? "Stop" : s.picking ? "Cancel" : "Start contextualizing";
  const status = h(
    "section",
    { class: "card status", "aria-label": "Contextualizing" },
    h(
      "div",
      { class: "status-line" },
      s.recording ? h("span", { class: "rec-dot", "aria-hidden": "true" }) : h("span", { class: "ring", "aria-hidden": "true" }),
      s.recording
        ? h("h2", { "data-live": "", role: "timer" }, "Contextualizing  " + app.clock(app.elapsed()))
        : h("h2", null, s.picking ? "Pick a window or screen" : "Not contextualizing"),
      button(startText, () => call("recorder", { action: "toggle" }), { size: "large", prominent: true, help: `${startText} (${keys.record})` }),
    ),
  );
  if (s.recording) {
    const n = s.events || 0;
    const proj = s.project ? app.projectName(s.project) : null;
    const line = `${n} moment${n === 1 ? "" : "s"} saved` + (s.screens > 1 ? ` · ${s.screens} screens` : "") + (proj ? ` · ${proj}` : "");
    status.append(
      h(
        "div",
        { class: "status-sub" },
        h("p", null, line),
        button(s.listening ? "Done speaking" : "Speak a note", () => call("recorder", { action: "voice" })),
        s.several_screens ? button("Add a screen", () => call("recorder", { action: "add-screen" }), { disabled: !s.can_add_screen }) : null,
      ),
    );
  } else {
    const all = app.config.share_screens === "all";
    status.append(
      h("div", { class: "status-sub" }, h("p", null, all ? "Shares all your screens. Private apps are never read." : "Pick the screen or window you work on. Only that is read.")),
    );
  }

  // Numbers.
  const stat = (k, v, d) => h("div", { class: "stat", role: "group", "aria-label": `${k}: ${v}, ${d}` }, h("span", { class: "k", "aria-hidden": "true" }, k), h("span", { class: "v", "aria-hidden": "true" }, v), h("span", { class: "d", "aria-hidden": "true" }, d));
  const wc = app.weekCapsules;
  const stats = h(
    "section",
    { class: "card stats", "aria-label": "Your numbers" },
    stat("Today", Stats.duration(app.today.seconds), "contextualized"),
    h("span", { class: "vsep", "aria-hidden": "true" }),
    stat("This week", Stats.duration(app.week.seconds), `${wc} capsule${wc === 1 ? "" : "s"} made`),
    h("span", { class: "vsep", "aria-hidden": "true" }),
    stat("Streak", `${app.streak} day${app.streak === 1 ? "" : "s"}`, app.streak > 0 ? "in a row" : "starts today"),
  );

  // Today by project.
  const by = app.today.byProject;
  const pal = palette(by.map((p) => p.id));
  const most = by[0]?.seconds || 1;
  const byCard = h(
    "div",
    { class: "card" },
    by.length
      ? h("div", { class: "bars", role: "list", "aria-label": "Today by project" }, by.slice(0, 5).map((p) => timeBar(app.projectName(p.id), p.seconds, most, pal(p.id), !p.id, Stats.duration(p.seconds))))
      : h("div", { class: "empty-card" }, "Nothing yet today. Start contextualizing when you begin work."),
  );
  const byBlock = h(
    "section",
    { class: "block", "aria-labelledby": "h-by" },
    h("div", { class: "capbar" }, h("h2", { class: "caption", id: "h-by" }, "Today by project"), h("button", { class: "plain", type: "button", onclick: () => app.go("timeline") }, "See today's timeline")),
    byCard,
  );

  // Recent capsules.
  const recent = app.capsules.slice(0, 3);
  const recentCard = h("div", { class: "card", role: recent.length ? "list" : null });
  if (!recent.length) recentCard.append(h("div", { class: "empty-card" }, "None yet. At the end of the day, make today's capsules."));
  recent.forEach((c, i) => {
    if (i) recentCard.append(h("div", { class: "sep in46", "aria-hidden": "true" }));
    recentCard.append(capsuleRow(app, c));
  });
  const recentBlock = h(
    "section",
    { class: "block", "aria-labelledby": "h-recent" },
    h("div", { class: "capbar" }, h("h2", { class: "caption", id: "h-recent" }, "Recent capsules"), recent.length ? h("button", { class: "plain", type: "button", onclick: () => app.go("capsules") }, "See all") : null),
    recentCard,
  );

  const main = h("div", { class: "home-main" }, status, stats, byBlock, recentBlock);
  const cols = h("div", { class: "home-cols" }, main, side(app, keys));
  content.append(cols);
  return content;
}

function side(app, keys) {
  const s = app.state;
  const shortcuts = h(
    "section",
    { class: "card side-card", "aria-labelledby": "h-keys" },
    h("h3", { id: "h-keys" }, "Shortcuts"),
    h("div", { class: "keyline" }, h("kbd", { class: "keycap" }, keys.record), h("span", null, "Start or stop")),
    h("div", { class: "keyline" }, h("kbd", { class: "keycap" }, keys.voice), h("span", null, "Speak a note")),
  );

  const pinned = s.project_by === "you";
  const row = (title, ic, selected, action, muted) =>
    h(
      "button",
      { class: "pick-row" + (muted ? " muted" : ""), type: "button", role: muted ? null : "radio", "aria-checked": muted ? null : String(selected), onclick: action },
      icon(ic),
      h("span", { class: "t" }, title),
      selected ? icon("check", "check") : null,
    );
  const working = h(
    "section",
    { class: "card side-card working", "aria-labelledby": "h-working" },
    h("h3", { id: "h-working" }, "Working on"),
    h(
      "div",
      { role: "radiogroup", "aria-labelledby": "h-working" },
      row("Find by itself", "search", !pinned, () => call("recorder", { action: "project", id: null })),
      app.projects.map((p) => row(p.name, "folder", pinned && s.project === p.id, () => call("recorder", { action: "project", id: p.id }))),
    ),
    row("New project", "plus", false, () => call("recorder", { action: "new-project" }), true),
  );

  const kind = app.kind();
  const name = app.aiName();
  const enter = app.platform === "mac" ? "Return" : "Enter";
  const make = h("section", { class: "card side-card make", "aria-labelledby": "h-make" }, h("h3", { id: "h-make" }, "Make today's capsules"));
  if (app.auto?.agent || app.auto?.running) {
    // a tool on this computer makes them in the background (SPEC "Automatic capsules")
    const c = Auto.control(app.auto, app.now(), (ms) => app.dates.time(ms));
    if (c.kind === "button") make.append(h("p", null, Auto.homeLine(app.auto)));
    make.append(makeControl(app, { full: true }));
  } else if (AI.usesCopy(kind)) {
    make.append(h("p", null, AI.makeLine(kind, name, app.ai.dayLine, enter)));
    make.append(
      button("Copy my day", () => actions.makeTodays(app), { size: "large", prominent: true, full: true, disabled: app.ai.working }),
      button("Save from clipboard", () => actions.saveFromClipboard(app), { size: "large", full: true, disabled: app.ai.working }),
    );
  } else {
    make.append(h("p", null, AI.makeLine(kind, name, app.ai.dayLine, enter)));
    make.append(button("Make capsules", () => actions.makeTodays(app), { size: "large", prominent: true, full: true }));
  }
  make.append(h("div", { class: "your-ai" }, h("span", null, app.auto?.agent ? `With ${app.auto.agent.name}` : AI.homeLine(kind, name)), h("button", { class: "link", type: "button", onclick: () => app.go("settings") }, "Change")));

  return h("div", { class: "home-side" }, shortcuts, working, make);
}
