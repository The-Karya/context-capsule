// context-capsule — ui: the Timeline page (SPEC "Timeline") — one day: the day control and calendar, the time recorded,
// search, the day bar coloured by project, where your time went, and each recording folded to one line. Opened, a
// recording lists its stretches, voice/typed notes and what wasn't read; Show what was read lists the reads.
// Delete a read, a stretch or a recording (after a confirm); make a capsule from one recording.

import { h, icon, button, iconButton, menu, popover, alert } from "../dom.js";
import { call, why } from "../host.js";
import * as T from "../core/timeline.js";
import { dayKey, parseKey, addDays } from "../core/dates.js";
import * as actions from "../actions.js";

export function mount(root, app, opts = {}) {
  const v = {
    day: dayKey(app.now()),
    query: "",
    data: { blocks: [] },
    loaded: false,
    opened: new Set(),
    closed: new Set(),
    reads: new Set(), // stretches with Show what was read open
    full: new Set(), // reads with Show all
    days: new Set(),
    openFirst: opts.open === "first",
    gen: 0,
  };
  const today = () => dayKey(app.now());
  const isToday = () => v.day === today();
  const searching = () => T.words(v.query).length > 0;

  // ---------- header (built once, so the search field keeps its focus) ----------

  const back = h("button", { class: "icon-btn chev", type: "button", title: "The day before", "aria-label": "The day before", onclick: () => step(-1) }, icon("chevronLeft"));
  const title = h("button", { class: "day-title", type: "button", title: "Pick a day", "aria-haspopup": "dialog" });
  const next = h("button", { class: "icon-btn chev", type: "button", title: "The next day", "aria-label": "The next day", onclick: () => step(1) }, icon("chevronRight"));
  const todayBtn = button("Today", () => show(today()));
  const control = h("div", { class: "day-control", role: "group", "aria-label": "Day" }, back, title, next, todayBtn);
  const summary = h("p", { class: "tl-sum", "aria-live": "polite" });
  const field = h("input", { type: "search", placeholder: "Search this day", "aria-label": "Search this day", spellcheck: false, autocomplete: "off" });
  field.addEventListener("input", () => {
    v.query = field.value;
    drawBody();
  });
  field.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && field.value) {
      field.value = "";
      v.query = "";
      drawBody();
    }
  });
  const head = h("div", { class: "tl-head" }, control, summary, h("div", { class: "search", role: "search" }, icon("search"), field));
  const body = h("div", { class: "tl-body" });
  const content = h("div", { class: "content tl" }, head, body);
  content.style.gap = "0";
  root.append(content);
  title.addEventListener("click", () => calendar(title));

  function drawHead() {
    const long = app.dates.long(parseKey(v.day), app.now());
    title.textContent = long;
    title.setAttribute("aria-label", `${long}. Pick a day`);
    next.disabled = isToday();
    todayBtn.hidden = isToday();
    summary.textContent = v.loaded ? T.summary(v.data) : " ";
  }

  // ---------- reading ----------

  async function load() {
    const gen = ++v.gen;
    const ids = app.sessionIds;
    const files = await app.sessionFiles(T.candidates(ids, v.day));
    if (gen !== v.gen) return;
    const live = app.state.recording ? app.state.session_id : null;
    v.data = T.loadDay(v.day, files, { projects: app.projects, recording: live });
    v.days = T.recordedDays(ids);
    v.loaded = true;
    if (v.openFirst && v.data.blocks[0]) {
      v.opened.add(v.data.blocks[0].session);
      v.openFirst = false;
    }
    drawHead();
    drawBody();
  }

  function show(key) {
    if (key > today()) return;
    if (key === v.day) return;
    v.day = key;
    v.opened.clear();
    v.closed.clear();
    v.reads.clear();
    v.full.clear();
    v.loaded = false;
    v.data = { blocks: [] };
    drawHead();
    drawBody();
    load();
  }
  const step = (n) => show(addDays(v.day, n));

  // ---------- the page below the header ----------

  const isOpen = (b) => (v.opened.has(b.session) ? true : v.closed.has(b.session) ? false : searching() ? b.entries.length > 0 : b.recording);

  function drawBody(target) {
    const keep = root.scrollTop;
    const out = [];
    const blocks = T.filterDay(v.data, v.query);
    const stretches = T.stretchesOf(v.data);
    const sh = T.shares(stretches);
    const pal = T.palette(sh.rows.map((r) => r.key));
    const colour = (s) => pal(T.paletteKey(s, sh.byProject));

    if (v.loaded && (!blocks.length || (searching() && blocks.every((b) => !b.entries.length)))) {
      out.push(h("p", { class: "tl-empty" }, T.emptyLine({ searching: searching(), today: isToday(), recordKey: app.keys().record })));
    }
    if (v.loaded && stretches.length) {
      out.push(dayBar(stretches, colour), went(sh, pal));
    }
    if (blocks.length) out.push(h("h2", { class: "caption rec-caption", id: "h-recs" }, "Recordings"));
    for (const b of blocks) out.push(recording(b, colour));
    body.replaceChildren(...out);
    root.scrollTop = keep;
    if (target) {
      const el = body.querySelector(`[data-key="${CSS.escape(target)}"]`);
      el?.scrollIntoView({ block: "start", behavior: "smooth" });
      el?.querySelector("button")?.focus({ preventScroll: true });
    }
  }

  function dayBar(stretches, colour) {
    const range = T.barRange(stretches);
    const total = Math.max(1, range.to - range.from);
    const track = h("div", { class: "track", role: "group", "aria-label": "The day, by project" });
    for (const s of stretches) {
      const x = ((s.start - range.from) / total) * 100;
      const w = (Math.max(0, s.end - s.start) / total) * 100;
      const label = `${app.dates.span(s.start, s.end)} · ${s.name}`;
      const c = colour(s);
      track.append(
        h("button", {
          class: "seg " + (c < 0 ? "cn" : "c" + c),
          type: "button",
          title: label,
          "aria-label": label,
          style: { left: `min(${x}%, calc(100% - 3px))`, width: w + "%" },
          onclick: () => reveal(s),
        }),
      );
    }
    const hours = h("div", { class: "hours", "aria-hidden": "true" }, T.barTicks(range).map((t) => h("span", { style: { left: t.at * 100 + "%" } }, app.dates.hour(t.t))));
    return h("div", { class: "daybar" }, track, hours);
  }

  function went(sh, pal) {
    const most = Math.max(0, ...sh.rows.map((r) => r.seconds));
    const label = (k) => (k ? (sh.byProject ? app.projectName(k) : k) : sh.byProject ? "Not sorted" : "Other");
    const rows = sh.rows.map((r) => {
      const name = label(r.key);
      const d = T.length(r.seconds);
      const c = pal(r.key);
      return h(
        "div",
        { class: "bar-row", role: "listitem", "aria-label": `${name}: ${d}` },
        h("span", { class: "name" + (r.key ? "" : " none"), "aria-hidden": "true" }, name),
        h("span", { class: "track", "aria-hidden": "true" }, h("span", { class: "fill " + (c < 0 ? "cn" : "c" + c), style: { width: (most ? (r.seconds / most) * 100 : 0) + "%" } })),
        h("span", { class: "dur", "aria-hidden": "true" }, d),
      );
    });
    return h(
      "section",
      { class: "went-block", "aria-labelledby": "h-went" },
      h("h2", { class: "caption", id: "h-went" }, "Where your time went"),
      h("div", { class: "tl-panel bars went", role: "list", "aria-labelledby": "h-went" }, rows),
    );
  }

  function reveal(s) {
    v.closed.delete(s.session);
    v.opened.add(s.session);
    drawBody("s-" + s.id);
  }

  function recording(b, colour) {
    const full = v.data.blocks.find((x) => x.session === b.session) || b;
    const open = isOpen(b);
    const n = T.names(full);
    const names = n.byProject ? n.keys.map((k) => app.projectName(k)) : n.keys;
    const line = T.blockLine(full, app.dates.span(full.start, full.end), names);
    const disc = h(
      "button",
      { class: "disc", type: "button", "aria-expanded": String(open), onclick: () => toggle(b) },
      icon("disclosure"),
      full.recording ? h("span", { class: "dot7 rec-dot7", title: "Recording now", "aria-hidden": "true" }) : null,
      h("span", { class: "line" }, line),
      full.recording ? h("span", { class: "now" }, "Recording now") : null,
    );
    const more = iconButton("ellipsisCircle", "More", (e) => menu(e.currentTarget, [{ text: "Delete this recording…", destructive: true, action: () => del("block", full) }]));
    more.setAttribute("aria-haspopup", "menu");
    const headRow = h(
      "div",
      { class: "rec-head" },
      disc,
      button("Make a capsule", () => actions.makeCapsule(app, b.session, v.day), { disabled: app.ai.working, help: "Your AI makes a capsule from this recording." }),
      more,
    );
    const panel = h("section", { class: "rec tl-panel", "aria-label": line, "data-key": "b-" + b.session }, headRow);
    if (!open) return panel;
    if (!b.entries.length) {
      panel.append(h("div", { class: "nothing-row" }, T.nothingInBlock(searching())));
      return panel;
    }
    const list = h("div", { role: "list" });
    for (const e of b.entries) {
      if (e.type === "stretch") list.append(stretchRow(e.stretch, colour));
      else if (e.type === "note") list.append(noteRow(e.moment));
      else list.append(gapRow(e.gap));
    }
    list.lastElementChild?.classList.add("last");
    panel.append(list);
    return panel;
  }

  function toggle(b) {
    if (isOpen(b)) {
      v.opened.delete(b.session);
      v.closed.add(b.session);
    } else {
      v.closed.delete(b.session);
      v.opened.add(b.session);
    }
    drawBody();
    body.querySelector(`[data-key="b-${CSS.escape(b.session)}"] .disc`)?.focus({ preventScroll: true });
  }

  function rowMenu(title, action) {
    const b = iconButton("ellipsis", "More", (e) => menu(e.currentTarget, [{ text: title, destructive: true, action }]), "fade");
    b.setAttribute("aria-haspopup", "menu");
    return b;
  }

  function stretchRow(s, colour) {
    const c = colour(s);
    const open = v.reads.has(s.id);
    const project = s.project ? app.projectName(s.project) : null;
    const details = [s.where, project].filter(Boolean).join(" · ");
    const when = `${app.dates.span(s.start, s.end)} · ${T.length(s.seconds)}`;
    const del_ = () => del("stretch", s);
    const row = h(
      "div",
      {
        class: "entry stretch hoverfade",
        role: "listitem",
        "data-key": "s-" + s.id,
        oncontextmenu: (e) => {
          e.preventDefault();
          menu(row, [{ text: "Delete this stretch…", destructive: true, action: del_ }]);
        },
      },
      icon("window", "appicon"),
      h(
        "div",
        { class: "main" },
        h("div", { class: "top" }, h("span", { class: "dot7 " + (c < 0 ? "cn" : "c" + c), "aria-hidden": "true" }), h("b", null, s.name), h("span", { class: "when" }, when)),
        details ? h("div", { class: "where", title: details }, details) : null,
        s.peek ? h("div", { class: "peek selectable" }, s.peek) : null,
        h(
          "div",
          { class: "reads" },
          h("span", null, T.readsLine(s)),
          h("button", { class: "link", type: "button", "aria-expanded": String(open), onclick: () => toggleReads(s) }, open ? "Hide what was read" : "Show what was read"),
        ),
      ),
      rowMenu("Delete this stretch…", del_),
    );
    row.setAttribute("aria-label", `${s.name}, ${when}${details ? ", " + details : ""}`);
    if (!open) return row;
    const wrap = h("div", { role: "listitem" }, row);
    row.removeAttribute("role");
    for (const m of s.found ?? s.reads) wrap.append(readRow(m));
    return wrap;
  }

  function toggleReads(s) {
    if (v.reads.has(s.id)) v.reads.delete(s.id);
    else v.reads.add(s.id);
    drawBody();
    body.querySelector(`[data-key="s-${CSS.escape(s.id)}"] .reads .link`)?.focus({ preventScroll: true });
  }

  function readRow(m) {
    const all = v.full.has(m.id);
    const meta = m.lookedAt || m.where
      ? h(
          "div",
          { class: "meta" },
          m.lookedAt ? h("span", { class: "looked", title: "The mouse was over it on another screen. You weren't typing in it." }, "Looked at") : null,
          m.where ? h("span", { class: "where", title: m.where }, m.where) : null,
        )
      : null;
    let text;
    if (all) {
      text = [h("div", { class: "txt selectable" }, m.text), h("button", { class: "link", type: "button", onclick: () => { v.full.delete(m.id); drawBody(); } }, "Show less")];
    } else if (m.preview) {
      text = [
        h("div", { class: "txt fold selectable" }, m.preview),
        m.more ? h("button", { class: "link", type: "button", onclick: () => { v.full.add(m.id); drawBody(); } }, "Show all") : null,
      ];
    } else text = h("div", { class: "nothing" }, "Nothing new on screen");
    return h(
      "div",
      {
        class: "read-row hoverfade",
        oncontextmenu: (e) => {
          e.preventDefault();
          menu(e.currentTarget, [{ text: T.deleteMomentMenu(m), destructive: true, action: () => del("moment", m) }]);
        },
      },
      h("span", { class: "time" }, app.dates.span(m.t, m.end)),
      h("div", { class: "main" }, meta, text),
      rowMenu(T.deleteMomentMenu(m), () => del("moment", m)),
    );
  }

  function noteRow(m) {
    const title = T.deleteMomentMenu(m);
    return h(
      "div",
      {
        class: "entry note-row hoverfade",
        role: "listitem",
        oncontextmenu: (e) => {
          e.preventDefault();
          menu(e.currentTarget, [{ text: title, destructive: true, action: () => del("moment", m) }]);
        },
      },
      icon(m.kind === "voice" ? "mic" : "keyboard"),
      h("div", { class: "main" }, h("div", { class: "bubble selectable" }, m.preview || "…"), h("span", { class: "cap" }, T.noteCaption(m, app.dates.time(m.t)))),
      rowMenu(title, () => del("moment", m)),
    );
  }

  function gapRow(g) {
    return h("div", { class: "entry gap-row", role: "listitem" }, icon("eyeSlash"), h("span", null, T.gapLine(g, app.dates.span(g.t, g.end))));
  }

  // ---------- delete (after a confirm) ----------

  async function del(what, x) {
    const pc = app.thisComputer();
    let words, args;
    if (what === "moment") {
      words = T.deleteMomentText(x, pc);
      args = { session: x.session, lines: [{ t: x.stamp, line: x.line }] };
    } else if (what === "stretch") {
      words = T.deleteStretchText(x, app.dates.span(x.start, x.end), pc);
      args = { session: x.session, lines: x.reads.map((m) => ({ t: m.stamp, line: m.line })) };
    } else {
      words = T.deleteBlockText(x, app.dates.time(x.start), app.dates.time(x.end), pc);
      args = { session: x.session };
    }
    const i = await alert({ title: words.title, message: words.message, buttons: [{ text: "Delete", kind: "destructive" }, { text: "Cancel", kind: "cancel" }] });
    if (i !== 0) return;
    try {
      await call("timeline.delete", args);
    } catch (e) {
      const t = T.refusedText(why(e));
      await alert({ title: t.title, message: t.message });
    }
    app.files.delete(x.session);
    await app.loadSessions();
    await load();
    app.loadStats().then(() => app.refresh("stats"));
  }

  // ---------- the month calendar ----------

  function calendar(anchor) {
    const sel = parseKey(v.day);
    let year = new Date(sel).getFullYear();
    let month = new Date(sel).getMonth() + 1;
    const first = app.hello.firstWeekday || 1;
    popover(anchor, (close) => {
      const box = h("div", { class: "calendar" });
      const draw = (focusKey) => {
        const now = new Date(app.now());
        const thisMonth = year === now.getFullYear() && month === now.getMonth() + 1;
        const t = today();
        const grid = h("div", { class: "cal-grid", role: "grid", "aria-label": app.dates.monthTitle(year, month) });
        for (const d of app.dates.weekdays(first)) grid.append(h("span", { class: "wd", "aria-hidden": "true" }, d));
        for (const d of app.dates.monthCells(year, month, first)) {
          if (d == null) {
            grid.append(h("span", { "aria-hidden": "true" }));
            continue;
          }
          const key = `${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
          const dot = v.days.has(key);
          const long = app.dates.long(parseKey(key), app.now());
          grid.append(
            h(
              "button",
              {
                class: "cal-day" + (dot ? " dot" : "") + (key === t ? " today" : ""),
                type: "button",
                "data-key": key,
                "aria-pressed": String(key === v.day),
                "aria-label": long + (dot ? ", recorded" : ""),
                title: dot ? "Recorded this day" : "",
                disabled: key > t,
                tabindex: key === (focusKey || v.day) ? "0" : "-1",
                onclick: () => {
                  close();
                  show(key);
                },
              },
              String(d),
              h("i", { "aria-hidden": "true" }),
            ),
          );
        }
        box.replaceChildren(
          h(
            "div",
            { class: "cal-head" },
            h("b", { "aria-live": "polite" }, app.dates.monthTitle(year, month)),
            h("button", { class: "icon-btn", type: "button", title: "The month before", "aria-label": "The month before", onclick: () => move(-1) }, icon("chevronLeft")),
            h("button", { class: "icon-btn", type: "button", title: "The next month", "aria-label": "The next month", disabled: thisMonth, onclick: () => move(1) }, icon("chevronRight")),
          ),
          grid,
        );
        const f = grid.querySelector(`[data-key="${focusKey || v.day}"]`) || grid.querySelector(".cal-day:not(:disabled)");
        if (f) {
          f.tabIndex = 0;
          if (focusKey !== null) f.focus();
        }
        grid.addEventListener("keydown", (e) => {
          const k = e.target.dataset?.key;
          const d = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[e.key];
          if (!k || !d) return;
          e.preventDefault();
          const n = addDays(k, d);
          if (n > today()) return;
          const [y, m] = n.split("-").map(Number);
          if (y !== year || m !== month) {
            year = y;
            month = m;
          }
          draw(n);
        });
      };
      const move = (n) => {
        month += n;
        if (month < 1) (month = 12), year--;
        if (month > 12) (month = 1), year++;
        draw(null);
      };
      draw();
      return box;
    }, { label: "Pick a day" });
  }

  // ---------- start, and staying up to date ----------

  drawHead();
  const ready = load();
  // Today updates by itself every few seconds while recording, keeping what is open and the scroll position.
  const live = setInterval(() => {
    if (app.state.recording && isToday() && app.state.session_id) {
      app.files.delete(app.state.session_id);
      load();
    }
  }, 4000);

  return {
    ready,
    refresh(what) {
      if (what === "sessions" || what === "projects") load();
      else if (what === "ai" || what === "keys") drawBody();
    },
    show(o) {
      if (o?.open === "first") {
        v.openFirst = true;
        load();
      }
    },
    unmount() {
      clearInterval(live);
    },
  };
}
