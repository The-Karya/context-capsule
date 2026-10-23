// context-capsule — ui: Projects — the projects work is sorted into; rename one, change how it's found, delete it

import { h, button, alert } from "../dom.js";
import { call } from "../host.js";
import * as P from "../core/projects.js";

export function mount(root, app) {
  let editing = null; // project id

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
    const found = p.match.length ? "Found by: " + p.match.join(", ") : "Not found automatically yet";
    return h(
      "div",
      { class: "proj-row", role: "listitem" },
      h("div", { class: "main" }, h("b", null, p.name), h("span", null, found)),
      button("Edit", () => {
        editing = p.id;
        draw();
      }, { label: `Edit ${p.name}` }),
      button("Delete", async () => {
        const i = await alert({
          title: `Delete “${p.name}”?`,
          message: "Its capsules stay. New work won't be sorted into it anymore.",
          buttons: [{ text: "Delete", kind: "destructive" }, { text: "Cancel", kind: "cancel" }],
        });
        if (i === 0) await save(P.remove(app.projects, p.id));
      }, { label: `Delete ${p.name}` }),
    );
  }

  function draw() {
    const content = h(
      "div",
      { class: "content" },
      h("h1", { class: "title" }, "Projects"),
      h("p", { class: "body" }, "Work is sorted into these by itself, from words in window names and web addresses."),
    );
    if (!app.projects.length) {
      content.append(h("p", { class: "body" }, "No projects yet. Your AI makes them when it sorts your day."));
    } else {
      const card = h("div", { class: "card", role: "list", "aria-label": "Projects" });
      app.projects.forEach((p, i) => {
        if (i) card.append(h("div", { class: "sep", "aria-hidden": "true" }));
        card.append(row(p));
      });
      content.append(card);
    }
    root.replaceChildren(content);
  }

  draw();
  return {
    refresh(what) {
      if (what === "projects" && !editing) draw();
    },
  };
}
