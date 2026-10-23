// context-capsule — ui: Capsules — search your capsules, By day (a date rail) or By project (remembered: config
// capsules_view), share or open one; the header has the make control (Make today's capsules, or Automatic capsules'
// status) and ••• (open a capsule file, Copy my day / Save capsules from clipboard for an AI on the web)

import { h, icon, button, iconButton, menu, segmented } from "../dom.js";
import { call } from "../host.js";
import * as Caps from "../core/capsules.js";
import * as AI from "../core/yourai.js";
import * as Auto from "../core/auto.js";
import * as actions from "../actions.js";

/** The make control (Capsules header, Home): Make today's capsules, a quiet status line with a dot (Automatic
 * capsules: "Next at 6:00 PM" + Make now, "Made at …", "Couldn't make them" + Try again), or "Making today's
 * capsules…" with a spinner while a run goes. `opts.full`: Home's version (wide button, the line with its detail). */
export function makeControl(app, opts = {}) {
  const c = Auto.control(app.auto, app.now(), (ms) => app.dates.time(ms));
  if (c.kind === "running" && opts.full) {
    const b = button("", null, { cls: "soft", disabled: true, size: "large", full: true });
    b.append(h("span", { class: "spinner", "aria-hidden": "true" }), c.text);
    return h("div", { role: "status" }, b);
  }
  if (c.kind === "running") {
    return h("div", { class: "make-status", role: "status" }, h("span", { class: "spinner", "aria-hidden": "true" }), h("span", null, c.text));
  }
  if (c.kind === "line") {
    const state = c.action === "make" ? "next" : c.action === "retry" ? "failed" : "done";
    const line = h(
      "div",
      { class: "make-status" + (opts.full ? " full" : ""), role: "status", title: !opts.full && c.detail ? c.detail : null },
      opts.full
        ? h("span", null, c.detail ? `${c.text}. ${c.detail}` : c.text)
        : [h("span", { class: "dot " + state, "aria-hidden": "true" }), h("span", null, c.text)],
    );
    const act = c.action ? button(c.actionText, () => actions.makeTodays(app), { size: opts.full ? "large" : null, cls: opts.full ? "soft" : "", full: opts.full }) : null;
    return opts.full ? h("div", { class: "side-sec make" }, line, act) : [line, act];
  }
  if (opts.full) return button(Auto.words.makeCapsules, () => actions.makeTodays(app), { cls: "soft", size: "large", full: true, disabled: app.ai.working });
  return button(Auto.words.make, () => actions.makeTodays(app), { prominent: true, disabled: app.ai.working });
}

/** ••• on the Capsules page. */
function moreMenu(app, anchor) {
  menu(anchor, [
    { text: "Open a capsule file…", action: () => actions.pickCapsule(app) },
    "-",
    { heading: "Using ChatGPT or another AI on the web?" },
    { text: "Copy my day", action: () => actions.copyDay(app, null), disabled: app.ai.working },
    { text: "Save capsules from clipboard", action: () => actions.saveFromClipboard(app), disabled: app.ai.working },
  ], "More");
}

/** The project as a small tag: its colour dot and name. */
function tag(app, id) {
  return h("span", { class: "tag" }, h("span", { class: "dot " + app.projectColour(id), "aria-hidden": "true" }), app.projectName(id));
}

/**
 * One capsule row: its name, In short, and Share / Open (shown on hover or focus); click or Return opens it.
 * `look`: "project" (By project: the day on the right), "day" (By day: the title and the project as a tag), "recent"
 * (Home: the day's name, the project as a tag).
 */
export function capsuleRow(app, c, look = "project") {
  const now = app.now();
  let title, withTag, when, lead = null;
  if (look === "day") {
    const r = Caps.dayRow(c, (id) => app.projectName(id));
    title = r.title;
    withTag = r.tag;
    if (!r.tag && c.project) lead = h("span", { class: "dot " + app.projectColour(c.project), "aria-hidden": "true" });
    when = "";
  } else {
    title = Caps.label(c, app.dates, now);
    withTag = look === "recent" && !!c.project;
    when = Caps.when(c, app.dates, now);
  }
  const shareItems = actions.shareItems(app, c);
  const shareBtn = iconButton("share", "Share…", () => actions.openShare(app, c), "fade");
  const row = h(
    "div",
    {
      class: "cap-row hoverfade",
      role: "listitem",
      tabindex: "0",
      onclick: (e) => {
        if (!e.target.closest("button")) app.read({ file: c.file });
      },
      onkeydown: (e) => {
        if ((e.key === "Enter" || e.key === " ") && e.target === row) {
          e.preventDefault();
          app.read({ file: c.file });
        }
      },
      oncontextmenu: (e) => {
        e.preventDefault();
        menu(row, [{ text: "Open", action: () => app.read({ file: c.file }) }, ...shareItems.filter((x) => x !== "-")], "Capsule");
      },
    },
    h(
      "div",
      { class: "main" },
      h("div", { class: "t" }, h("b", null, lead, title), withTag ? tag(app, c.project) : null, c.draft ? h("small", null, "Draft") : null),
      c.inShort ? h("div", { class: "s" }, c.inShort) : null,
    ),
    when ? h("span", { class: "when" }, when) : null,
    h("div", { class: "acts" }, shareBtn, iconButton("eye", "Open", () => app.read({ file: c.file }), "fade")),
  );
  const project = withTag || lead ? app.projectName(c.project) : "";
  row.setAttribute("aria-label", [title, project !== title ? project : "", c.draft ? "Draft" : "", c.inShort, when].filter(Boolean).join(", "));
  return row;
}

export function mount(root, app) {
  let query = "";
  let view = Caps.view(app.config);
  const content = h("div", { class: "content" });
  const field = h("input", { type: "search", placeholder: "Search your capsules", "aria-label": "Search your capsules", spellcheck: false, autocomplete: "off" });
  field.addEventListener("input", () => {
    query = field.value;
    drawList();
  });
  field.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && field.value) {
      field.value = "";
      field.dispatchEvent(new Event("input"));
    }
  });

  const make = h("div", { class: "make-slot" });
  const more = iconButton("ellipsis", "More", (e) => moreMenu(app, e.currentTarget), "more-btn");
  more.setAttribute("aria-haspopup", "menu");
  const top = h("div", { class: "page-head" }, h("h1", { class: "title" }, "Capsules"), make, more);
  const views = segmented([["day", "By day"], ["project", "By project"]], view, (v) => {
    view = v;
    const c = { ...app.config };
    if (v === "day") delete c[Caps.VIEW_KEY];
    else c[Caps.VIEW_KEY] = v;
    app.config = c;
    call("config.set", { key: Caps.VIEW_KEY, value: v === "day" ? null : v }).catch(() => {});
    drawList();
  }, "Show capsules");
  const tools = h("div", { class: "tools" }, h("div", { class: "search", role: "search" }, icon("search"), field), views);
  const list = h("div", { class: "group-list", "aria-live": "polite" });
  content.append(top, tools, list);
  root.append(content);

  function drawMake() {
    make.replaceChildren(...[makeControl(app)].flat().filter(Boolean));
  }

  function drawList() {
    const out = [];
    const name = (id) => app.projectName(id);
    if (!app.capsules.length) {
      out.push(h("p", { class: "empty" }, `No capsules yet. Start contextualizing (${app.keys().record}), then make today's capsules.`));
    } else if (view === "day") {
      const days = Caps.byDay(app.capsules, query, name);
      if (!days.length) out.push(h("p", { class: "empty" }, "No titles match. Your AI can search inside every capsule."));
      for (const d of days) {
        const g = d.day ? app.dates.gutter(d.day, app.now()) : { top: "Other", sub: "" };
        const id = "d-" + Math.random().toString(36).slice(2);
        out.push(
          h(
            "section",
            { class: "day", "aria-labelledby": id },
            h("h2", { class: "dl", id, "aria-label": [g.top, g.sub].filter(Boolean).join(", ") }, h("b", null, g.top), g.sub),
            h("div", { class: "rail", role: "list", "aria-labelledby": id }, d.items.map((c) => capsuleRow(app, c, "day"))),
          ),
        );
      }
    } else {
      const groups = Caps.groups(app.capsules, query, name);
      if (!groups.length) out.push(h("p", { class: "empty" }, "No titles match. Your AI can search inside every capsule."));
      for (const g of groups) {
        const id = "g-" + Math.random().toString(36).slice(2);
        const p = g.items[0]?.project;
        const n = g.items.length;
        out.push(
          h(
            "section",
            { class: "group" },
            h("h2", { class: "proj-h", id }, h("span", { class: "dot " + app.projectColour(p), "aria-hidden": "true" }), g.title, h("small", null, `· ${n} capsule${n === 1 ? "" : "s"}`)),
            h("div", { class: "list", role: "list", "aria-labelledby": id }, g.items.map((c) => capsuleRow(app, c, "project"))),
          ),
        );
      }
    }
    list.replaceChildren(...out);
  }

  drawMake();
  drawList();
  // "Next at 6:00 PM" turns into the button when the time comes
  const timer = setInterval(drawMake, 30_000);
  return {
    refresh(what) {
      drawMake();
      if (what !== "auto" && what !== "ai") drawList();
    },
    unmount() {
      clearInterval(timer);
    },
  };
}
