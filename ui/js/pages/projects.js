// context-capsule — ui: Projects — the projects work is sorted into; rename one, change how it's found, delete it
// (asked in the row itself)

import { h, button } from "../dom.js";
import { call } from "../host.js";
import * as P from "../core/projects.js";

export function mount(root, app) {
  let editing = null; // project id
  let deleting = null; // project id asking "Delete …?"

  async function save(list) {
    app.projects = list;
    await call("projects.save", { text: P.toJSON(list) }).catch(() => {});
    draw();
  }

  function row(p) {
    if (editing === p.id) {
      const name = h("input", { class: "text fill", value: p.name, "aria-label": "Name", placeholder: "Name" });
      const clues = h("input", {
        class: "text fill",
        value: p.match.join(", "),
        "aria-label": "Found by",
        placeholder: "Found by (comma-separated words in window titles or web addresses)",
      });
      const done = async () => {
        let list = P.rename(app.projects, p.id, name.value);
        list = P.withClues(list, p.id, clues.value);
        editing = null;
        await save(list);
      };
      const cancel = () => {
        editing = null;
        draw();
      };
      const box = h(
        "div",
        { class: "proj-edit", role: "group", "aria-label": `Edit ${p.name}` },
        name,
        clues,
        h("div", { class: "btns" }, button("Cancel", cancel), button("Save", done, { prominent: true })),
      );
      box.addEventListener("keydown", (e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          done();
        } else if (e.key === "Escape") cancel();
      });
      setTimeout(() => name.focus());
      return box;
    }
    if (deleting === p.id) {
      const cancel = () => {
        deleting = null;
        draw();
        root.querySelector(`[data-id="${CSS.escape(p.id)}"] .del`)?.focus();
      };
      const box = h(
        "div",
        { class: "confirm", role: "group", "aria-label": `Delete “${p.name}”?` },
        h("div", { class: "main" }, h("b", null, `Delete “${p.name}”?`), h("span", null, "Its capsules stay. New work won't be sorted into it anymore.")),
        h("div", { class: "btns" }, button("Cancel", cancel), button("Delete", async () => {
          deleting = null;
          await save(P.remove(app.projects, p.id));
        }, { cls: "danger" })),
      );
      box.addEventListener("keydown", (e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          cancel();
        }
      });
      setTimeout(() => box.querySelector("button")?.focus());
      return box;
    }
    const found = p.match.length ? "Found by: " + p.match.join(", ") : "Not found automatically yet";
    return h(
      "div",
      { class: "proj-row hoverfade", role: "listitem", "data-id": p.id },
      h("span", { class: "dot " + app.projectColour(p.id), "aria-hidden": "true" }),
      h("div", { class: "main" }, h("b", null, p.name), h("span", null, found)),
      h(
        "div",
        { class: "btns" },
        button("Edit", () => {
          editing = p.id;
          deleting = null;
          draw();
        }, { label: `Edit ${p.name}`, cls: "fade" }),
        button("Delete", () => {
          deleting = p.id;
          editing = null;
          draw();
        }, { label: `Delete ${p.name}`, cls: "fade del" }),
      ),
    );
  }

  function draw() {
    const content = h(
      "div",
      { class: "content" },
      h("div", { class: "page-head" }, h("h1", { class: "title" }, "Projects")),
      h("p", { class: "lede" }, "Work is sorted into these by itself, from words in window names and web addresses."),
    );
    if (!app.projects.length) {
      content.append(h("p", { class: "empty" }, "No projects yet. Your AI makes them when it sorts your day."));
    } else {
      content.append(h("div", { class: "list", role: "list", "aria-label": "Projects" }, app.projects.map((p) => row(p))));
    }
    root.replaceChildren(content);
  }

  draw();
  return {
    refresh(what) {
      if (what === "projects" && !editing && !deleting) draw();
    },
  };
}
