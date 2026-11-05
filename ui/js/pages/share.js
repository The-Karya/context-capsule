// context-capsule — ui: sharing a capsule — no dialog: the capsule as it will be shared, with what might be private
// marked where it is (hidden: struck through in orange; kept: a dashed underline; click one to change just it), the
// kinds with switches on the side, and Share copy (the system share sheet on a Mac, else Copy file). The copy starts
// with a two-line note on what it is and how to use it, for someone without Context Capsule. SPEC "Sharing a capsule".

import { h, menu } from "../dom.js";
import * as MD from "../core/markdown.js";
import * as Check from "../core/sharecheck.js";
import * as Caps from "../core/capsules.js";
import * as actions from "../actions.js";

const KIND = Object.fromEntries(Check.KINDS.map((k) => [k.kind, k]));

/** "3 things hidden", "1 thing hidden", "Nothing hidden", "Nothing personal found". */
export function countLine(found, hidden) {
  if (!found) return "Nothing personal found";
  if (!hidden) return "Nothing hidden";
  return hidden === 1 ? "1 thing hidden" : `${hidden} things hidden`;
}

/** opts: { file: "capsules/…" }. */
export function mount(root, app, opts = {}) {
  const view = opts.file ? opts : app.sharing || {};
  app.sharing = view;
  const content = h("div", { class: "content share-page" });
  root.append(content);

  // What was found, and what is hidden now (item text → hidden?).
  let text = null;
  let groups = [];
  let items = [];
  const kindOf = new Map();
  const hidden = new Set();
  const marks = new Map(); // item → its highlighted places
  let switches = [];
  let status = null;
  let doc = null;

  async function draw() {
    const file = await actions.readCapsule(view.file);
    // what is shared, and shown here: the capsule with the note on top
    text = file == null ? null : Caps.withShareNote(file);
    const saved = app.capsules.find((c) => c.file === view.file);
    if (text == null || !saved) {
      content.replaceChildren(bar(saved?.title || "", false), h("p", { class: "body" }, "This capsule isn't there any more."));
      return;
    }
    groups = Check.find(text);
    items = groups.flatMap((g) => g.items);
    for (const g of groups) {
      for (const x of g.items) {
        kindOf.set(x, g.kind);
        if (g.on) hidden.add(x);
      }
    }
    marks.clear();
    doc = h("article", { class: "share-text reader-text selectable", "aria-label": "The copy you share" },
      render(Check.body(text.replace(/\r\n/g, "\n"))));
    roving();
    content.replaceChildren(
      bar(saved.title, true),
      h("div", { class: "share-grid" }, kinds(), doc),
    );
    update();
  }

  // ---------- the top bar ----------

  function bar(title, ok) {
    const back = h("button", { class: "back", type: "button", onclick: () => app.backFromShare(), "aria-label": "Back to " + app.shareBackTitle() },
      h("span", { class: "chev", "aria-hidden": "true" }, "‹"), "Back");
    status = h("span", { class: "share-count", role: "status", "aria-live": "polite" });
    const go = h("button", {
      class: "btn prominent", type: "button", disabled: !ok,
      title: "Shares a copy with the hidden things replaced by [removed]. Your capsule stays the same.",
      onclick: () => share(app.platform === "mac" ? "more" : "copy-file"),
    }, "Share copy");
    const more = h("button", {
      class: "btn more", type: "button", disabled: !ok, "aria-label": "Other ways to share the copy", "aria-haspopup": "menu",
      title: "Other ways to share the copy",
      onclick: (e) => menu(e.currentTarget, [
        ...(app.platform === "mac" ? [{ text: "Copy file", action: () => share("copy-file") }] : []),
        { text: "Copy text", action: () => share("copy-text") },
        { text: "Save a copy…", action: () => share("save-copy") },
      ], "Share the copy"),
    }, "•••");
    return h("header", { class: "share-bar" },
      back,
      h("h1", { class: "share-title" }, `Share “${title}”`),
      ok ? status : null,
      ok ? more : null,
      ok ? go : null);
  }

  function share(how) {
    const saved = app.capsules.find((c) => c.file === view.file);
    if (!saved || text == null) return;
    return actions.share(app, saved, how, hidden.size ? Check.shareCopy(text, items, hidden) : text);
  }

  // ---------- the kinds ----------

  function kinds() {
    switches = [];
    const list = h("ul", { class: "share-kinds-list", role: "list" });
    for (const g of groups) {
      const id = "share-k-" + g.kind;
      const sw = h("button", {
        class: "switch", type: "button", role: "switch", "aria-labelledby": id,
        disabled: !g.items.length,
        onclick: () => {
          const all = g.items.every((x) => hidden.has(x));
          for (const x of g.items) all ? hidden.delete(x) : hidden.add(x);
          update();
        },
      });
      switches.push([g, sw]);
      list.append(h("li", { class: "share-kind" + (g.items.length ? "" : " none") },
        h("span", { id, class: "share-kind-name" }, g.title,
          h("span", { class: "share-kind-n tnum", "aria-label": ` (${g.items.length})` }, String(g.items.length))),
        sw));
    }
    if (!items.length) {
      return h("aside", { class: "share-kinds", "aria-label": "What was checked" },
        h("p", { class: "share-kinds-note" }, "Checked for names, emails, phone numbers, links, money, companies and file paths. "
          + "The copy is your capsule as it is."));
    }
    return h("aside", { class: "share-kinds", "aria-label": "What to hide" },
      h("p", { class: "share-kinds-note" }, "Hide in the copy. Click any marked word to change just it."),
      list);
  }

  // ---------- the text, with each item marked ----------

  function mark(seg) {
    const kind = kindOf.get(seg.item);
    const el = h("span", {
      class: "find",
      role: "button",
      tabindex: "-1",
      "data-kind": kind,
      onclick: () => toggle(seg.item),
      onkeydown: (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          toggle(seg.item);
        }
      },
    }, seg.text);
    if (!marks.has(seg.item)) marks.set(seg.item, []);
    marks.get(seg.item).push(el);
    return el;
  }

  function pieces(s) {
    return Check.segments(s, items).map((seg) => (seg.item ? mark(seg) : seg.text));
  }

  function inline(s) {
    return MD.inline(s).map((p) => {
      if (p.t === "b") return h("strong", null, pieces(p.text));
      if (p.t === "i") return h("em", null, pieces(p.text));
      if (p.t === "code") return h("code", null, pieces(p.text));
      // Links stay text here: what you see is what you share.
      if (p.t === "link") return p.text === p.url ? pieces(p.url) : [pieces(p.text), " (", pieces(p.url), ")"];
      return pieces(p.text);
    });
  }

  function render(md) {
    const out = [];
    for (const b of MD.blocks(md)) {
      if (b.type === "h") out.push(h(b.level <= 2 ? "h2" : "h3", null, inline(b.text)));
      else if (b.type === "p") out.push(h("p", null, inline(b.text)));
      else if (b.type === "ul" || b.type === "ol") out.push(h(b.type, null, b.items.map((t) => h("li", null, inline(t)))));
      else if (b.type === "quote") out.push(h("blockquote", null, inline(b.text)));
      else if (b.type === "code") out.push(h("pre", null, h("code", null, pieces(b.text))));
    }
    return out;
  }

  function toggle(item) {
    hidden.has(item) ? hidden.delete(item) : hidden.add(item);
    update();
  }

  // One Tab stop in the text: arrows move between the marked words, Home / End to the first / last.
  function roving() {
    const all = () => [...doc.querySelectorAll(".find")];
    const first = all()[0];
    if (first) first.tabIndex = 0;
    doc.addEventListener("keydown", (e) => {
      const list = all();
      const i = list.indexOf(document.activeElement);
      if (i < 0) return;
      let j = -1;
      if (e.key === "ArrowRight" || e.key === "ArrowDown") j = Math.min(list.length - 1, i + 1);
      else if (e.key === "ArrowLeft" || e.key === "ArrowUp") j = Math.max(0, i - 1);
      else if (e.key === "Home") j = 0;
      else if (e.key === "End") j = list.length - 1;
      if (j < 0) return;
      e.preventDefault();
      list[i].tabIndex = -1;
      list[j].tabIndex = 0;
      list[j].focus();
    });
  }

  function update() {
    for (const [item, els] of marks) {
      const off = hidden.has(item);
      const one = KIND[kindOf.get(item)]?.one || "";
      for (const el of els) {
        el.classList.toggle("hidden", off);
        el.classList.toggle("kept", !off);
        el.setAttribute("aria-pressed", String(off));
        el.setAttribute("aria-label", `${item}, ${one}, ${off ? "hidden in the copy" : "kept in the copy"}`);
        el.title = off ? "Hidden in the copy. Click to keep it." : "Kept in the copy. Click to hide it.";
      }
    }
    for (const [g, sw] of switches) {
      const on = g.items.length ? g.items.every((x) => hidden.has(x)) : g.on;
      sw.setAttribute("aria-checked", String(on));
    }
    if (status) status.textContent = countLine(items.length > 0, hidden.size);
  }

  const keys = (e) => {
    if (e.key === "Escape" && !document.querySelector(".scrim, .popover")) {
      e.preventDefault();
      app.backFromShare();
    }
  };
  document.addEventListener("keydown", keys);
  const ready = draw().then(() => content.querySelector(".back")?.focus({ preventScroll: true }));
  return {
    ready,
    unmount() {
      document.removeEventListener("keydown", keys);
    },
  };
}
