// context-capsule — ui: Capsules — search your capsules and every capsule by project (share, open); the header has
// the make control (Make today's capsules, or Automatic capsules' status) and ••• (open a capsule file, Copy my day /
// Save capsules from clipboard for an AI on the web)

import { h, icon, button, iconButton, menu } from "../dom.js";
import * as Caps from "../core/capsules.js";
import * as AI from "../core/yourai.js";
import * as Auto from "../core/auto.js";
import * as actions from "../actions.js";

/** The make control (Capsules header, Home): the blue Make today's capsules button, a quiet status line (Automatic
 * capsules), or "Making today's capsules…" with a spinner while a run goes. `opts.full`: Home's wide button. */
export function makeControl(app, opts = {}) {
  const c = Auto.control(app.auto, app.now(), (ms) => app.dates.time(ms));
  const size = opts.full ? { size: "large", full: true } : {};
  if (c.kind === "running" && opts.full) {
    const b = button("", null, { prominent: true, disabled: true, ...size });
    b.append(h("span", { class: "spinner", "aria-hidden": "true" }), c.text);
    return h("div", { role: "status" }, b);
  }
  if (c.kind === "running") {
    return h("div", { class: "make-status", role: "status" }, h("span", { class: "spinner", "aria-hidden": "true" }), h("span", null, c.text));
  }
  if (c.kind === "line") {
    return h(
      "div",
      { class: "make-status" + (opts.full ? " full" : ""), role: "status", title: !opts.full && c.detail ? c.detail : null },
      h("span", null, opts.full && c.detail ? `${c.text}. ${c.detail}` : c.text),
      c.action ? h("button", { class: "link", type: "button", onclick: () => actions.makeTodays(app) }, c.actionText) : null,
    );
  }
  return button(opts.full ? Auto.words.makeCapsules : Auto.words.make, () => actions.makeTodays(app), { prominent: true, disabled: app.ai.working, ...size });
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

/** One capsule: icon, its name (the day, "Whole project" or its title), In short, and Share / Open. */
export function capsuleRow(app, c) {
  const now = app.now();
  const label = Caps.label(c, app.dates, now);
  const when = Caps.when(c, app.dates, now);
  const shareItems = actions.shareItems(app, c);
  const shareBtn = iconButton("share", "Share", (e) => menu(e.currentTarget, shareItems, "Share"), "fade");
  shareBtn.setAttribute("aria-haspopup", "menu");
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
    icon(c.kind === "project" ? "folder" : "doc"),
    h(
      "div",
      { class: "main" },
      h("div", { class: "t" }, h("b", null, label), c.draft ? h("small", null, "Draft") : null),
      c.inShort ? h("div", { class: "s" }, c.inShort) : null,
    ),
    when ? h("span", { class: "when" }, when) : null,
    h("div", { class: "acts" }, shareBtn, iconButton("eye", "Open", () => app.read({ file: c.file }), "fade")),
  );
  row.setAttribute("aria-label", [label, c.draft ? "Draft" : "", c.inShort, when].filter(Boolean).join(", "));
  return row;
}

export function mount(root, app) {
  let query = "";
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
  const more = iconButton("ellipsisCircle", "More", (e) => moreMenu(app, e.currentTarget), "more-btn");
  more.setAttribute("aria-haspopup", "menu");
  const top = h("div", { class: "title-row" }, h("h1", { class: "title" }, "Capsules"), make, more);
  const search = h("div", { class: "search-row", role: "search" }, h("div", { class: "search" }, icon("search"), field));
  const list = h("div", { class: "group-list", "aria-live": "polite" });
  content.append(top, search, list);
  root.append(content);

  function drawMake() {
    make.replaceChildren(makeControl(app));
  }

  function drawList() {
    const out = [];
    if (!app.capsules.length) {
      out.push(h("p", { class: "body" }, `No capsules yet. Start contextualizing (${app.keys().record}), then make today's capsules.`));
    } else {
      const groups = Caps.groups(app.capsules, query, (id) => app.projectName(id));
      if (!groups.length) out.push(h("p", { class: "body" }, "No titles match. Your AI can search inside every capsule."));
      for (const g of groups) {
        const id = "g-" + Math.random().toString(36).slice(2);
        const card = h("div", { class: "card", role: "list", "aria-labelledby": id });
        g.items.forEach((c, i) => {
          if (i) card.append(h("div", { class: "sep in46", "aria-hidden": "true" }));
          card.append(capsuleRow(app, c));
        });
        out.push(h("section", { class: "group" }, h("h2", { class: "caption", id }, g.title), card));
      }
    }
    list.replaceChildren(...out);
  }

  drawMake();
  drawList();
  // "Next: today at 6:00 PM" turns into the button when the time comes
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
