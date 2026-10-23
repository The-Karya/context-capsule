// context-capsule — ui: one capsule, to read — its title, day and project, its sections from the Markdown, and what you
// can do with it: Continue in your AI, Share…, Show in Finder. Opened from a capsule row (click or the eye), and for a
// capsule opened from Finder / Explorer / Files, dropped on the window or picked with "Open a capsule…".

import { h, button } from "../dom.js";
import { call } from "../host.js";
import * as Caps from "../core/capsules.js";
import * as AI from "../core/yourai.js";
import * as MD from "../core/markdown.js";
import { body } from "../core/sharecheck.js";
import * as actions from "../actions.js";

/** Inline Markdown as elements (text nodes only: nothing in a capsule becomes HTML). Links open in the browser. */
function inline(text) {
  return MD.inline(text).map((p) => {
    if (p.t === "b") return h("strong", null, p.text);
    if (p.t === "i") return h("em", null, p.text);
    if (p.t === "code") return h("code", null, p.text);
    if (p.t === "link") {
      return h("a", {
        href: p.url,
        title: p.url,
        onclick: (e) => {
          e.preventDefault();
          call("open.url", { url: p.url }).catch(() => {});
        },
      }, p.text);
    }
    return p.text;
  });
}

function render(md) {
  const out = [];
  for (const b of MD.blocks(md)) {
    if (b.type === "h") out.push(h(b.level <= 2 ? "h2" : "h3", null, inline(b.text)));
    else if (b.type === "p") out.push(h("p", null, inline(b.text)));
    else if (b.type === "ul" || b.type === "ol") out.push(h(b.type, null, b.items.map((t) => h("li", null, inline(t)))));
    else if (b.type === "quote") out.push(h("blockquote", null, inline(b.text)));
    else if (b.type === "code") out.push(h("pre", null, h("code", null, b.text)));
  }
  return out;
}

/** opts: { file: "capsules/…" } for a saved capsule, or { path, text } for one opened from outside. */
export function mount(root, app, opts = {}) {
  const view = opts.file || opts.path ? opts : app.reading || {};
  app.reading = view;
  const content = h("div", { class: "content reader" });
  root.append(content);

  async function draw() {
    let text = view.text;
    if (text == null && view.file) text = (await actions.readCapsule(view.file)) ?? null;
    const saved = view.file ? app.capsules.find((c) => c.file === view.file) : null;
    const name = (view.file || view.path || "").split(/[\\/]/).pop();
    if (text == null) {
      content.replaceChildren(backRow(), h("p", { class: "body" }, "This capsule isn't there any more."));
      return;
    }
    const c = saved || Caps.parse(text, name, view.path);
    const now = app.now();
    const day = /^\d{4}-\d{2}-\d{2}$/.test(c.date) ? app.dates.shortDay(c.date, now) : Caps.when({ ...c, kind: "" }, app.dates, now);
    const meta = [
      day,
      c.project ? app.projectName(c.project) : "",
      c.author && !saved ? `From ${c.author}` : "",
      c.draft ? "Draft" : "",
    ].filter(Boolean).join(" · ");

    const kind = app.kind();
    const aiName = app.aiName();
    const go = AI.usesCopy(kind) ? (kind.type === "website" ? `Continue in ${aiName}` : "Copy for your AI") : `Continue in ${aiName}`;
    const acts = [button(go, () => actions.continueCapsule(app, { name, path: view.path, text, saved: !!saved }), {
      prominent: true,
      help: AI.usesCopy(kind) ? "Copies the capsule to paste into your AI's chat." : `${aiName} opens and carries on from this capsule.`,
    })];
    if (saved) {
      acts.push(button("Share…", () => actions.openShare(app, saved), { help: "See what's in the copy you share, and hide what's private." }), button(Caps.showLabel(app.platform), () => actions.share(app, saved, "reveal")));
    }

    content.replaceChildren(
      backRow(),
      h("header", { class: "reader-head" },
        h("h1", { class: "title" }, c.title),
        meta ? h("p", { class: "meta" }, c.project ? h("span", { class: "dot " + app.projectColour(c.project), "aria-hidden": "true" }) : null, meta) : null,
        h("div", { class: "reader-acts" }, acts)),
      h("article", { class: "reader-text selectable", "aria-label": c.title }, render(body(text.replace(/\r\n/g, "\n")))),
    );
  }

  function backRow() {
    const back = h("button", { class: "back", type: "button", onclick: () => app.back() },
      h("span", { class: "chev", "aria-hidden": "true" }, "‹"), app.backTitle());
    return h("nav", { class: "reader-nav", "aria-label": "Capsule" }, back);
  }

  const keys = (e) => {
    if (e.key === "Escape" && !document.querySelector(".scrim, .popover")) {
      e.preventDefault();
      app.back();
    }
  };
  document.addEventListener("keydown", keys);
  const ready = draw().then(() => content.querySelector(".back")?.focus({ preventScroll: true }));
  return {
    ready,
    refresh(what) {
      if (what === "capsules" || what === "clients" || what === "config" || what === "ai") draw();
    },
    unmount() {
      document.removeEventListener("keydown", keys);
    },
  };
}
