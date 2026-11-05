// context-capsule — ui: Settings (and the first run) — your name, set up, Your AI, shortcuts, screens, look, every day,
// voice notes, never record, updates. Every change is saved at once (config.json through the host).

import { h, icon, button, menu, segmented } from "../dom.js";
import { call, why } from "../host.js";
import * as AI from "../core/yourai.js";
import * as Auto from "../core/auto.js";
import * as Keys from "../core/shortcuts.js";
import * as actions from "../actions.js";
import { appIcon } from "../appicon.js";

/** The set-up steps' words, by platform (the host only says which steps there are and which are done). */
function stepWords(id, platform) {
  const mac = platform === "mac";
  switch (id) {
    case "read":
      return { icon: "viewfinder", title: "Read your apps", button: "Turn On", help: "Clean text, window names and web addresses, so projects are found by themselves." };
    case "voice":
      return mac
        ? { icon: "mic", title: "Voice notes", button: "Allow", help: "Say what you're doing and why. Turned into text on this Mac; audio is never saved." }
        : { icon: "mic", title: "Voice notes", button: "Get", help: "A one-time download (about 150 MB) so speech becomes text on this computer. Audio is never saved." };
    case "dictation":
      return { icon: "waveform", title: "Dictation", button: "Turn On", help: "Apple needs it for voice notes. Scroll down to Dictation and switch it on." };
    case "login":
      return { icon: "power", title: "Open at login", button: "Turn On", help: mac ? "Always ready in the menu bar." : "Always ready in the tray." };
    default:
      return null;
  }
}

let ids = 0;
const uid = (p) => `${p}-${++ids}`;

/** A labelled row: title on the left, the control on the right. */
function row(title, control, id = uid("r")) {
  if (control && !control.id && /^(INPUT|SELECT|BUTTON)$/.test(control.tagName)) control.setAttribute("aria-labelledby", id);
  const inner = control?.querySelector?.("select, input");
  if (inner && !inner.getAttribute("aria-label")) inner.setAttribute("aria-labelledby", id);
  return h("div", { class: "row" }, h("span", { class: "label", id }, title), control);
}

/** A pop-up menu; `frame`: the width the Mac gives it (the pop-up sits in the middle of it). */
function popup(options, value, onchange, frame) {
  const sel = h("select", { onchange: (e) => onchange(e.target.value) }, options.map(([v, label]) => h("option", { value: v, selected: String(v) === String(value) }, label)));
  const p = h("span", { class: "popup" }, sel, icon("updown"));
  return frame ? h("span", { class: "frame", style: { width: frame + "px" } }, p) : p;
}

/** "At 6:00 PM": the time as the system writes it, with up/down arrows (and ↑ ↓ keys) a quarter of an hour at a time. */
function timeStepper(app, hhmm, onchange) {
  let [hh, mm] = hhmm.split(":").map(Number);
  const val = h("span", { class: "val", tabindex: "0", role: "spinbutton", "aria-label": "At" });
  const show = () => {
    const d = new Date(2026, 0, 1, hh, mm);
    val.textContent = app.dates.time(d.getTime());
    val.setAttribute("aria-valuetext", val.textContent);
    val.setAttribute("aria-valuenow", String(hh * 60 + mm));
  };
  let t = 0;
  const step = (minutes) => {
    const total = (((hh * 60 + mm + minutes) % 1440) + 1440) % 1440;
    hh = Math.floor(total / 60);
    mm = total % 60;
    show();
    clearTimeout(t);
    t = setTimeout(() => onchange(`${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}`), 400);
  };
  val.addEventListener("keydown", (e) => {
    const d = { ArrowUp: 15, ArrowDown: -15, PageUp: 60, PageDown: -60 }[e.key];
    if (d) {
      e.preventDefault();
      step(d);
    }
  });
  show();
  return h(
    "span",
    { class: "stepper" },
    val,
    h(
      "span",
      { class: "arrows" },
      h("button", { type: "button", tabindex: "-1", "aria-label": "Later", onclick: () => step(15) }, icon("chevronUp")),
      h("button", { type: "button", tabindex: "-1", "aria-label": "Earlier", onclick: () => step(-15) }, icon("chevronDown")),
    ),
  );
}

function toggle(on, onchange, label) {
  const b = h("button", { class: "switch", type: "button", role: "switch", "aria-checked": String(!!on), "aria-label": label });
  b.addEventListener("click", () => {
    const next = b.getAttribute("aria-checked") !== "true";
    b.setAttribute("aria-checked", String(next));
    onchange(next);
  });
  return b;
}

const sep = (cls = "") => h("div", { class: "sep " + cls, "aria-hidden": "true" });

export function mount(root, app) {
  const set = (key, value) => {
    const c = { ...app.config };
    if (value == null) delete c[key];
    else c[key] = value;
    app.config = c;
    return call("config.set", { key, value: value ?? null }).catch(() => {});
  };
  let moreOpen = false;
  let listening = null; // "record" | "voice" while waiting for keys
  const problems = { record: null, voice: null };
  let appNames = {};

  const content = h("div", { class: "content" });
  root.append(content);
  const parts = {};
  const section = (name, build) => {
    const el = h("div", { class: "section", style: { display: "contents" } });
    parts[name] = { el, build };
    el.append(...[build()].flat());
    content.append(el);
  };
  const redraw = (name) => {
    const p = parts[name];
    if (p) p.el.replaceChildren(...[p.build()].flat());
  };

  content.append(h("div", { class: "page-head" }, h("h1", { class: "title" }, "Settings")));

  // You
  section("you", () => {
    const name = h("input", { class: "text", type: "text", value: typeof app.config.author === "string" ? app.config.author : app.hello.userName || "", placeholder: "Name", spellcheck: false });
    let t = 0;
    name.addEventListener("input", () => {
      clearTimeout(t);
      t = setTimeout(() => {
        const clean = name.value.trim();
        set("author", clean || null);
      }, 300);
    });
    return [
      h("h2", { class: "caption" }, "You"),
      h("div", { class: "rows" }, row("Your name", name)),
      h("p", { class: "note" }, "Shown on Home, and written on your capsules as the author."),
    ];
  });

  // Set up
  section("setup", () => {
    const card = h("div", { class: "rows" });
    const steps = app.setup.steps || [];
    steps.forEach((s, i) => {
      if (i) card.append(sep("in48"));
      if (s.id === "ai") return card.append(...aiRows(app, s.done));
      const w = stepWords(s.id, app.platform);
      if (!w) return;
      const tid = uid("st");
      card.append(
        h(
          "div",
          { class: "step", title: w.help },
          icon(w.icon),
          h("span", { class: "label", id: tid }, w.title),
          s.done
            ? h("span", { role: "img", "aria-label": "Done" }, icon("check", "done"))
            : button(w.button, async () => {
                await call("setup.do", { id: s.id }).catch(() => {});
                await app.loadSetup();
                redraw("setup");
              }, { label: `${w.button}: ${w.title}`, help: w.help }),
        ),
      );
    });
    return [h("h2", { class: "caption" }, "Set up"), card];
  });

  function aiRows(app, done) {
    const out = [];
    const installed = app.clients.filter((c) => c.installed);
    const missing = app.clients.filter((c) => !c.installed);
    out.push(
      h(
        "div",
        { class: "step", title: AI.words.connectHelp },
        icon("link"),
        h("span", { class: "label" }, AI.words.connectYourAI),
        done ? h("span", { role: "img", "aria-label": "Done" }, icon("check", "done")) : !app.clientsLoaded ? h("span", { class: "note" }, "…") : null,
      ),
    );
    for (const c of installed) {
      out.push(sep("in48"));
      let right;
      if (app.ai.busy === c.id) right = h("span", { class: "note" }, "…");
      else if (c.connected) {
        const more = h("button", { class: "icon-btn", type: "button", title: "More", "aria-label": `More for ${c.name}`, "aria-haspopup": "menu" }, icon("ellipsis"));
        more.addEventListener("click", () => menu(more, [{ text: AI.words.disconnect, action: () => actions.disconnect(app, c) }]));
        right = [h("span", { role: "img", "aria-label": "Connected" }, icon("check", "done")), more];
      } else right = button(AI.words.connect, () => actions.connect(app, c), { size: "small", disabled: app.ai.busy != null, label: `Connect ${c.name}` });
      out.push(h("div", { class: "ai-row" }, appIcon(c.name, "", "terminal", "app"), h("span", { class: "label" }, c.name), right));
    }
    if (missing.length) {
      out.push(sep("in48"));
      const more = h("button", { class: "more-row", type: "button", "aria-expanded": String(moreOpen) }, icon("disclosure"), h("span", null, AI.words.more));
      more.addEventListener("click", () => {
        moreOpen = !moreOpen;
        redraw("setup");
        parts.setup.el.querySelector(".more-row")?.focus();
      });
      out.push(more);
      if (moreOpen) for (const c of missing) out.push(h("div", { class: "missing-row" }, h("span", { class: "label" }, c.name), h("small", null, AI.words.notInstalled)));
    }
    if (app.ai.note) out.push(h("div", { class: "card-note", role: "status" }, app.ai.note));
    out.push(h("div", { class: "card-note" }, AI.words.anyOtherLine));
    return out;
  }

  // Your AI
  section("ai", () => {
    const kind = app.kind();
    const choice = app.choice();
    const opts = AI.choices(app.clients, choice).map((c) => [c.id, c.label]);
    return [
      h("h2", { class: "caption" }, "Your AI"),
      h("div", { class: "rows" }, row(AI.words.makeWith, popup(opts, choice, (v) => actions.setChoice(app, v), 240))),
      h("p", { class: "note" }, AI.settingsLine(kind, app.aiName())),
    ];
  });

  // Shortcuts
  section("keys", () => {
    const keys = app.keys();
    const one = (which) => {
      const title = Keys.title(which);
      const b = button(listening === which ? Keys.PRESS_KEYS : keys[which], () => (listening === which ? stop() : listen(which)), {
        label: listening === which ? `${title}: ${Keys.PRESS_HELP}` : `${title}: ${keys[which]}. Click to change.`,
        cls: listening === which ? "listening" : "",
      });
      if (listening === which) setTimeout(() => b.focus());
      return h(
        "div",
        { class: "short-row" },
        h("div", { class: "line" }, h("span", { class: "label" }, title), h("span", { class: "frame", style: { minWidth: "110px" } }, b)),
        problems[which] ? h("div", { class: "problem", role: "alert" }, problems[which]) : null,
      );
    };
    const changed = keys.raw && (Keys.text(keys.raw.record) !== Keys.defaults.record || Keys.text(keys.raw.voice) !== Keys.defaults.voice);
    return [
      h("h2", { class: "caption" }, "Shortcuts"),
      h("div", { class: "rows" }, one("record"), sep(), one("voice")),
      h(
        "div",
        { class: "notebar" },
        h("p", { class: "note" }, Keys.SHORTCUTS_HELP),
        changed ? h("button", { class: "plain", type: "button", onclick: () => saveKeys(Keys.defaults.record, Keys.defaults.voice) }, Keys.RESET) : null,
      ),
    ];
  });

  function listen(which) {
    problems.record = problems.voice = null;
    listening = which;
    call("shortcuts.listen", { on: true }).catch(() => {});
    document.addEventListener("keydown", onKey, true);
    redraw("keys");
  }
  function stop() {
    if (!listening) return;
    listening = null;
    document.removeEventListener("keydown", onKey, true);
    call("shortcuts.listen", { on: false }).catch(() => {});
    redraw("keys");
  }
  async function saveKeys(record, voice) {
    try {
      await call("shortcuts.set", { record, voice });
      app.config = { ...app.config, hotkeys: { record, voice } };
    } catch (e) {
      return why(e) === "taken" ? Keys.TAKEN : why(e);
    }
    redraw("keys");
    app.refresh("keys");
    return null;
  }
  async function onKey(e) {
    if (e.key === "Tab") return;
    e.preventDefault();
    e.stopPropagation();
    const which = listening;
    if (Keys.isCancel(e)) return stop();
    const next = Keys.fromKeyEvent(e, app.platform);
    if (!next) return; // only modifiers so far
    const k = app.keys().raw;
    const other = which === "record" ? k.voice : k.record;
    stop();
    const problem = Keys.check(next, other, app.platform, which);
    if (problem) {
      problems[which] = problem;
      return redraw("keys");
    }
    const text = Keys.text(next);
    problems[which] = await saveKeys(which === "record" ? text : Keys.text(k.record), which === "voice" ? text : Keys.text(k.voice));
    redraw("keys");
  }

  // Screens
  section("screens", () => {
    const all = app.config.share_screens === "all";
    const card = h(
      "div",
      { class: "rows" },
      row("When you start", popup([["pick", "Pick a screen or window"], ["all", "All my screens"]], all ? "all" : "pick", async (v) => {
        await set("share_screens", v === "all" ? "all" : null);
        await app.loadSetup();
        redraw("screens");
        app.refresh("config");
      }, 220)),
    );
    if (all && app.platform === "mac" && app.setup.screenRecording === false) {
      card.append(sep(), row("Allow Screen Recording, then quit and reopen", button("Open Settings", () => call("setup.do", { id: "screen-recording" }))));
    }
    return [
      h("h2", { class: "caption" }, "Screens"),
      card,
      h("p", { class: "note" }, all ? "Every screen is shared at once, with no picker. Private apps are still never read." : "Each time, you pick what's shared. Add more screens while contextualizing."),
    ];
  });

  // Look
  section("look", () => {
    const a = ["light", "dark"].includes(app.config.appearance) ? app.config.appearance : "system";
    return [
      h("h2", { class: "caption" }, "Look"),
      h(
        "div",
        { class: "rows" },
        row("Appearance", segmented([["system", "System"], ["light", "Light"], ["dark", "Dark"]], a, async (v) => {
          await set("appearance", v === "system" ? null : v);
          await app.loadConfig();
        }, "Appearance")),
      ),
    ];
  });

  // Every day
  section("day", () => {
    const r = app.config.remind_at;
    const remindOn = !(r === false || r === null);
    const at = typeof r === "string" && /^\d{1,2}:\d{2}$/.test(r) ? r.padStart(5, "0") : "18:00";
    const auto = app.config.make_capsules === "auto";
    const st = app.auto;
    const keep = Number.isFinite(app.config.keep_recordings_days) ? Math.max(0, app.config.keep_recordings_days) : 30;
    const keepOpts = [[7, "1 week"], [30, "30 days"], [90, "90 days"], [0, "Never"]];
    if (!keepOpts.some(([v]) => v === keep)) keepOpts.splice(3, 0, [keep, `${keep} days`]);
    const mode = popup([["auto", Auto.words.automatically], ["click", Auto.words.whenIClick]], auto ? "auto" : "click", async (v) => {
      if (v === "auto" && !remindOn) await set("remind_at", at);
      await set("make_capsules", v === "auto" ? "auto" : null);
      await app.loadAuto();
      redraw("day");
      app.refresh("auto");
    }, 180);
    const card = h("div", { class: "rows" }, row(Auto.words.makeCapsules, mode));
    if (auto) {
      const problem = Auto.settingsProblem(st);
      if (problem) {
        card.append(
          h(
            "div",
            { class: "card-note flush" },
            problem + " ",
            h("button", { class: "link", type: "button", onclick: () => call("open.url", { url: Auto.words.learnMoreUrl }).catch(() => {}) }, Auto.words.learnMore),
          ),
        );
      }
      card.append(sep(), row("At", timeStepper(app, at, (v) => set("remind_at", v).then(() => app.loadAuto()).then(() => app.refresh("auto")))));
    } else {
      card.append(
        sep(),
        row("Remind me", toggle(remindOn, async (on) => {
          await set("remind_at", on ? at : false);
          redraw("day");
        }, "Remind me")),
      );
      if (remindOn) card.append(sep(), row("At", timeStepper(app, at, (v) => set("remind_at", v))));
    }
    card.append(
      sep(),
      row("Delete old recordings after", popup(keepOpts, keep, (v) => set("keep_recordings_days", Number(v)))),
      sep(),
      row("Sounds", toggle(app.config.sounds !== false, (on) => set("sounds", on), "Sounds")),
    );
    const note = [Auto.settingsNote(auto ? st : null), "Capsules are never deleted."].filter(Boolean).join(" ");
    return [h("h2", { class: "caption" }, "Every day"), card, h("p", { class: "note" }, note)];
  });

  // Voice notes
  section("voice", () => {
    const langs = app.hello.voiceLanguages || [];
    const cur = typeof app.config.voice_locale === "string" ? app.config.voice_locale : "";
    const opts = [["", app.hello.voiceLanguageDefault || (app.platform === "mac" ? "Same as my Mac" : "Same as my computer")], ...langs.map((l) => [l.id, l.name])];
    if (cur && !langs.some((l) => l.id === cur)) opts.push([cur, cur]);
    return [h("h2", { class: "caption" }, "Voice notes"), h("div", { class: "rows" }, row("Language", popup(opts, cur, (v) => set("voice_locale", v || null), 240)))];
  });

  // Never record
  section("never", () => {
    const list = Array.isArray(app.config.exclude_apps) ? app.config.exclude_apps.filter((x) => typeof x === "string") : [];
    const card = h("div", { class: "rows" }, h("div", { class: "card-note top" + (list.length ? " more" : "") }, "Password managers, WhatsApp and Signal are never recorded. Add other apps here."));
    for (const id of list) {
      card.append(sep(), row(appNames[id] || id, button("Remove", () => set("exclude_apps", list.filter((x) => x !== id)).then(() => redraw("never")), { label: `Remove ${appNames[id] || id}` })));
    }
    card.append(
      sep(),
      row("Add an app", button("Choose…", async () => {
        const a = await call("apps.pick").catch(() => null);
        if (!a?.id || list.includes(a.id)) return;
        appNames[a.id] = a.name || a.id;
        await set("exclude_apps", [...list, a.id]);
        redraw("never");
      })),
    );
    return [h("h2", { class: "caption" }, "Never record"), card];
  });
  const excluded = Array.isArray(app.config.exclude_apps) ? app.config.exclude_apps : [];
  if (excluded.length) {
    call("apps.names", { ids: excluded })
      .then((n) => {
        appNames = n || {};
        redraw("never");
      })
      .catch(() => {});
  }

  // Updates
  section("updates", () => {
    const u = app.state.update || {};
    return [
      h("h2", { class: "caption" }, "Updates"),
      h(
        "div",
        { class: "rows" },
        row("Update automatically", toggle(app.config.auto_update !== false, (on) => set("auto_update", on), "Update automatically")),
        sep(),
        row(
          `Version ${app.hello.version || ""}`,
          u.ready ? button("Restart now", () => call("updates", { action: "restart" })) : button("Check for Updates", () => call("updates", { action: "check" }), { disabled: !!u.working }),
        ),
      ),
      h("p", { class: "note", role: "status" }, u.line || "New versions are signed and checked. Nothing about you is sent."),
    ];
  });

  // Set up is checked every second while Settings shows (permissions change in System Settings).
  let last = JSON.stringify(app.setup);
  const timer = setInterval(async () => {
    await app.loadSetup();
    const now = JSON.stringify(app.setup);
    if (now !== last) {
      last = now;
      redraw("setup");
    }
  }, 1000);
  let tick = 0;
  const clients = setInterval(() => {
    if (++tick % 10 === 0) app.loadClients().then(() => redraw("setup"));
  }, 1000);

  return {
    refresh(what) {
      if (what === "clients" || what === "ai") {
        redraw("setup");
        redraw("ai");
      } else if (what === "config") {
        for (const n of ["ai", "keys", "screens", "look", "day", "voice", "never"]) if (!parts[n].el.contains(document.activeElement)) redraw(n);
      } else if (what === "auto") {
        if (!parts.day.el.contains(document.activeElement)) redraw("day");
      } else if (what === "state") redraw("updates");
      else if (what === "layout" || what === "keys") redraw("keys");
    },
    unmount() {
      clearInterval(timer);
      clearInterval(clients);
      stop();
    },
  };
}
