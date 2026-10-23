// context-capsule — ui: small helpers to build the page: elements, menus, the alert sheet, the toast

import { icon } from "./icons.js";

/** h("div", { class: "x", onclick, "aria-label": "…", style: { width: "4px" } }, child, "text", [more]) */
export function h(tag, props, ...children) {
  const el = document.createElement(tag);
  if (props) {
    for (const [k, v] of Object.entries(props)) {
      if (v == null || v === false) continue;
      if (k === "class") el.className = v;
      else if (k === "style") Object.assign(el.style, v);
      else if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v);
      else if (k === "text") el.textContent = v;
      else if (k in el && !k.includes("-") && k !== "list" && k !== "role") el[k] = v;
      else el.setAttribute(k, v === true ? "" : String(v));
    }
  }
  add(el, children);
  return el;
}

function add(el, children) {
  for (const c of children) {
    if (c == null || c === false) continue;
    if (Array.isArray(c)) add(el, c);
    else el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
}

export { icon };

/** A regular (bordered) button. */
export function button(text, onclick, opts = {}) {
  const cls = ["btn", opts.size, opts.prominent && "prominent", opts.full && "full", opts.cls].filter(Boolean).join(" ");
  return h("button", { class: cls, type: "button", onclick, disabled: opts.disabled, title: opts.help, "aria-label": opts.label }, text);
}

/** A small icon button (share, eye, ellipsis): its words go to screen readers and the tooltip. */
export function iconButton(name, label, onclick, cls = "") {
  return h("button", { class: "icon-btn " + cls, type: "button", title: label, "aria-label": label, onclick }, icon(name));
}

const layer = () => document.getElementById("layer");

/** Closes what's open in the layer (menu, popover). */
let closeOpen = null;
export function closePopups() {
  if (closeOpen) closeOpen();
}

/**
 * A popover under `anchor` (the month calendar, menus). Esc or a click outside closes it; focus comes back.
 * `build(close)` makes its content.
 */
export function popover(anchor, build, { role = "dialog", label = "", align = "start" } = {}) {
  closePopups();
  const back = document.activeElement;
  const box = h("div", { class: "popover", role, "aria-label": label || null });
  const close = () => {
    box.remove();
    document.removeEventListener("mousedown", outside, true);
    document.removeEventListener("keydown", keys, true);
    window.removeEventListener("resize", close);
    closeOpen = null;
    anchor.setAttribute?.("aria-expanded", "false");
    if (back && document.contains(back)) back.focus({ preventScroll: true });
  };
  const outside = (e) => {
    if (!box.contains(e.target) && !anchor.contains(e.target)) close();
  };
  const keys = (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  };
  box.append(build(close));
  layer().append(box);
  const r = anchor.getBoundingClientRect();
  const w = box.offsetWidth, hgt = box.offsetHeight;
  let x = align === "end" ? r.right - w : r.left;
  x = Math.max(8, Math.min(x, window.innerWidth - w - 8));
  let y = r.bottom + 6;
  if (y + hgt > window.innerHeight - 8) y = Math.max(8, r.top - hgt - 6);
  box.style.left = x + "px";
  box.style.top = y + "px";
  anchor.setAttribute?.("aria-expanded", "true");
  setTimeout(() => {
    document.addEventListener("mousedown", outside, true);
    document.addEventListener("keydown", keys, true);
    window.addEventListener("resize", close);
  });
  closeOpen = close;
  return { box, close };
}

/** A menu of items [{ text, action, destructive }] or "-" for a line; arrow keys move, Return picks. */
export function menu(anchor, items, label = "More") {
  const { box } = popover(
    anchor,
    (close) => {
      const m = h("div", { class: "menu", role: "menu", "aria-label": label });
      for (const it of items) {
        if (it === "-") {
          m.append(h("hr", { role: "separator" }));
          continue;
        }
        m.append(
          h("button", {
            type: "button",
            role: "menuitem",
            class: it.destructive ? "destructive" : "",
            tabindex: "-1",
            onclick: () => {
              close();
              it.action();
            },
          }, it.text),
        );
      }
      m.addEventListener("keydown", (e) => {
        const list = [...m.querySelectorAll('[role="menuitem"]')];
        const i = list.indexOf(document.activeElement);
        if (e.key === "ArrowDown") list[(i + 1) % list.length].focus();
        else if (e.key === "ArrowUp") list[(i - 1 + list.length) % list.length].focus();
        else return;
        e.preventDefault();
      });
      return m;
    },
    { role: "presentation", align: "end" },
  );
  box.querySelector('[role="menuitem"]')?.focus();
}

/**
 * The alert sheet: a title, words, buttons. Resolves the index of the button pressed (Esc = the cancel one).
 * buttons: [{ text, kind: "default" | "destructive" | "cancel" }]; `extra`: more content (Check before sharing).
 */
export function alert({ title, message = "", buttons = [{ text: "OK", kind: "default" }], extra = null, wide = false }) {
  closePopups();
  return new Promise((resolve) => {
    const back = document.activeElement;
    const tid = "dlg-t-" + Math.random().toString(36).slice(2);
    const did = tid + "-d";
    const done = (i) => {
      scrim.remove();
      document.removeEventListener("keydown", keys, true);
      if (back && document.contains(back)) back.focus({ preventScroll: true });
      resolve(i);
    };
    const btns = buttons.map((b, i) =>
      h("button", {
        type: "button",
        class: "btn" + (b.kind === "default" ? " prominent" : b.kind === "destructive" ? " danger" : ""),
        onclick: () => done(i),
      }, b.text),
    );
    const dlg = h(
      "div",
      { class: "dialog" + (wide ? " wide" : ""), role: "alertdialog", "aria-modal": "true", "aria-labelledby": tid, "aria-describedby": message ? did : null },
      h("h2", { id: tid }, title),
      message ? h("p", { id: did }, message) : null,
      extra,
      h("div", { class: "btns" }, btns),
    );
    const scrim = h("div", { class: "scrim" }, dlg);
    const cancel = buttons.findIndex((b) => b.kind === "cancel");
    const keys = (e) => {
      if (e.key === "Escape") {
        e.preventDefault();
        done(cancel >= 0 ? cancel : buttons.length - 1);
      } else if (e.key === "Enter" && !(document.activeElement instanceof HTMLButtonElement)) {
        // Return presses the default button (a focused button gets Return the usual way).
        const d = buttons.findIndex((b) => b.kind === "default");
        if (d >= 0) {
          e.preventDefault();
          done(d);
        }
      } else if (e.key === "Tab") {
        const f = [...dlg.querySelectorAll("button, input, a[href], [tabindex]:not([tabindex='-1'])")];
        const i = f.indexOf(document.activeElement);
        e.preventDefault();
        f[(i + (e.shiftKey ? -1 : 1) + f.length) % f.length].focus();
      }
    };
    document.addEventListener("keydown", keys, true);
    layer().append(scrim);
    // Focus the safe button (Cancel) when there's a destructive one, else the default.
    const safe = buttons.some((b) => b.kind === "destructive") ? cancel : buttons.findIndex((b) => b.kind === "default");
    (btns[safe >= 0 ? safe : 0]).focus();
  });
}

let toastTimer = 0;
/** A short message at the bottom of the window. */
export function toast(text) {
  const t = document.getElementById("toast");
  t.textContent = text;
  t.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show"), 4000);
}
