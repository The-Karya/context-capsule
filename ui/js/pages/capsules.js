// context-capsule — ui: Capsules — search your capsules, ask your AI, open one someone sent, Copy my day / Save from
// clipboard, and every capsule by project (share, open)

import { h, icon, button, iconButton, menu } from "../dom.js";
import * as Caps from "../core/capsules.js";
import * as AI from "../core/yourai.js";
import * as actions from "../actions.js";

/** One capsule: icon, its name (the day, "Whole project" or its title), In short, and Share / Open. */
export function capsuleRow(app, c) {
  const now = app.now();
  const label = Caps.label(c, app.dates, now);
  const when = Caps.when(c, app.dates, now);
  const shareItems = [
    { text: "Copy file", action: () => actions.share(app, c, "copy-file") },
    { text: "Copy text", action: () => actions.share(app, c, "copy-text") },
    { text: "Save a copy…", action: () => actions.share(app, c, "save-copy") },
    { text: app.platform === "mac" ? "Show in Finder" : "Show in folder", action: () => actions.share(app, c, "reveal") },
  ];
  if (app.platform === "mac") shareItems.push("-", { text: "More…", action: () => actions.share(app, c, "more") });
  const shareBtn = iconButton("share", "Share", (e) => menu(e.currentTarget, shareItems, "Share"), "fade");
  shareBtn.setAttribute("aria-haspopup", "menu");
  const row = h(
    "div",
    {
      class: "cap-row hoverfade",
      role: "listitem",
      ondblclick: () => actions.openCapsule(c),
      oncontextmenu: (e) => {
        e.preventDefault();
        menu(row, [{ text: "Open", action: () => actions.openCapsule(c) }, ...shareItems.filter((x) => x !== "-")], "Capsule");
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
    h("div", { class: "acts" }, shareBtn, iconButton("eye", "Open", () => actions.openCapsule(c), "fade")),
  );
  row.setAttribute("aria-label", [label, c.draft ? "Draft" : "", c.inShort, when].filter(Boolean).join(", "));
  return row;
}

export function mount(root, app) {
  let query = "";
  const content = h("div", { class: "content" });
  const field = h("input", { type: "search", placeholder: "Search your capsules", "aria-label": "Search your capsules", spellcheck: false, autocomplete: "off" });
  const askBtn = button("Ask your AI", () => actions.ask(app, query), { help: "Your AI searches all your capsules for this." });
  field.addEventListener("input", () => {
    query = field.value;
    drawList();
    askBtn.disabled = !query.trim();
  });
  field.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && field.value) {
      field.value = "";
      field.dispatchEvent(new Event("input"));
    }
  });
  askBtn.disabled = true;

  const top = h(
    "div",
    { class: "title-row" },
    h("h1", { class: "title" }, "Capsules"),
    button("Open a capsule…", () => actions.pickCapsule(app), { help: "Open a capsule someone sent you. Your AI carries on from it." }),
    button("Make today's capsules", () => actions.makeTodays(app)),
  );
  const search = h("div", { class: "search-row", role: "search" }, h("div", { class: "search" }, icon("search"), field), askBtn);
  const web = h("div", { class: "web-row" });
  const list = h("div", { class: "group-list", "aria-live": "polite" });
  content.append(top, search, web, list);
  root.append(content);

  function drawWeb() {
    askBtn.textContent = AI.askButton(app.kind());
    web.replaceChildren(
      h("p", null, app.ai.dayLine ?? AI.words.onTheWeb),
      button("Copy my day", () => actions.copyDay(app, null), { disabled: app.ai.working, help: AI.words.copyMyDayHelp }),
      button("Save from clipboard", () => actions.saveFromClipboard(app), { disabled: app.ai.working, help: AI.words.saveHelp }),
    );
  }

  function drawList() {
    const out = [];
    if (!app.capsules.length) {
      out.push(h("p", { class: "body" }, `No capsules yet. Start contextualizing (${app.keys().record}), then make today's capsules.`));
    } else {
      const groups = Caps.groups(app.capsules, query, (id) => app.projectName(id));
      if (!groups.length) out.push(h("p", { class: "body" }, "No titles match. Ask your AI to look inside every capsule."));
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

  drawWeb();
  drawList();
  return {
    refresh() {
      drawWeb();
      drawList();
    },
  };
}
