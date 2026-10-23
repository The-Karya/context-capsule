// context-capsule — ui: your own shortcuts (config.json "hotkeys"): read, write, show and check them (SPEC.md, "Your own shortcuts").

// A shortcut is { ctrl, alt, shift, cmd, key }; key is "A"–"Z", "0"–"9" or "F1"–"F12".
// Cmd is Mac only. Written in the order Ctrl, Alt, Shift, Cmd ("Ctrl+Alt+Shift+Cmd+K").

export const defaults = { record: "Alt+C", voice: "Alt+V" };

// Words for Settings → Shortcuts.
export const START_OR_STOP = "Start or stop";
export const SPEAK_A_NOTE = "Speak a note";
export const RESET = "Reset";
export const PRESS_KEYS = "Press keys…";
export const PRESS_HELP = "Press the new keys. Esc cancels.";
export const SHORTCUTS_HELP = "Click a shortcut, then press the keys you want.";
export const TAKEN = "Another app uses these keys";

// "Start or stop" for "record", "Speak a note" for "voice".
export function title(which) {
  return which === "voice" ? SPEAK_A_NOTE : START_OR_STOP;
}

// "C", "7", "F5" (any case) -> the key as written; null if a shortcut can't use it.
function key(s) {
  s = s.trim();
  if (/^[A-Za-z0-9]$/.test(s)) return s.toUpperCase();
  const f = /^[Ff]([1-9]|1[0-2])$/.exec(s);
  return f ? "F" + f[1] : null;
}

// "Alt+C", "ctrl + shift + f5", "Option+Command+K", "⌥+⌘+X" -> a shortcut; null if it can't be read
// (no key, two keys, a modifier twice, an unknown name). Not checked for being allowed: see check().
export function parse(text) {
  if (typeof text !== "string") return null;
  const parts = text.split("+").map((p) => p.trim());
  const k = key(parts.pop());
  if (!k) return null;
  const sc = { ctrl: false, alt: false, shift: false, cmd: false, key: k };
  const names = {
    ctrl: "ctrl", control: "ctrl", "⌃": "ctrl",
    alt: "alt", option: "alt", opt: "alt", "⌥": "alt",
    shift: "shift", "⇧": "shift",
    cmd: "cmd", command: "cmd", "⌘": "cmd",
  };
  for (const p of parts) {
    const flag = names[p.toLowerCase()];
    if (!flag || sc[flag]) return null;
    sc[flag] = true;
  }
  return sc;
}

// As written in config.json and shown on Windows and Linux: "Ctrl+Alt+Shift+Cmd+K".
export function text(sc) {
  const mods = [sc.ctrl && "Ctrl", sc.alt && "Alt", sc.shift && "Shift", sc.cmd && "Cmd"].filter(Boolean);
  return [...mods, sc.key].join("+");
}

// As the Mac shows it: "⌃⌥⇧⌘K".
export function symbols(sc) {
  return (sc.ctrl ? "⌃" : "") + (sc.alt ? "⌥" : "") + (sc.shift ? "⇧" : "") + (sc.cmd ? "⌘" : "") + sc.key;
}

// "⌥C" on a Mac, "Alt+C" elsewhere. Takes a shortcut or its text.
export function format(sc, platform) {
  if (typeof sc === "string") sc = parse(sc);
  if (!sc) return "";
  return platform === "mac" ? symbols(sc) : text(sc);
}

export function same(a, b) {
  return !!a && !!b && text(a) === text(b);
}

// A key press while recording a shortcut -> a shortcut (still to be checked), or null:
// only modifiers so far, or a key a shortcut can't use. Off the Mac, the Windows / Super key is never a modifier.
export function fromKeyEvent(e, platform) {
  const code = e.code || "";
  let k = null;
  let m;
  if ((m = /^Key([A-Z])$/.exec(code))) k = m[1];
  else if ((m = /^Digit([0-9])$/.exec(code))) k = m[1];
  else if ((m = /^F([1-9]|1[0-2])$/.exec(code))) k = "F" + m[1];
  if (!k) return null;
  if (e.metaKey && platform !== "mac") return null;
  return { ctrl: !!e.ctrlKey, alt: !!e.altKey, shift: !!e.shiftKey, cmd: platform === "mac" && !!e.metaKey, key: k };
}

// Esc alone: stop recording, nothing changes.
export function isCancel(e) {
  return (e.code === "Escape" || e.key === "Escape") && !(e.ctrlKey || e.altKey || e.shiftKey || e.metaKey);
}

// Needs Ctrl, Alt or (on a Mac) Cmd, unless the key is F1–F12. Off the Mac, Cmd is never allowed.
function needsModifier(sc, mac) {
  if (sc.cmd && !mac) return true;
  return !(sc.ctrl || sc.alt || sc.cmd || /^F\d/.test(sc.key));
}

// Can `next` be used next to `other`? null if so, else the short line under the row.
// `which`: the one being changed ("record" | "voice"); the line names the other one.
export function check(next, other, platform, which = "record") {
  const mac = platform === "mac";
  if (typeof next === "string") next = parse(next);
  if (typeof other === "string") other = parse(other);
  if (!next || needsModifier(next, mac)) return mac ? "Add ⌃, ⌥ or ⌘" : "Add Ctrl or Alt"; // the Mac app's words
  if (same(next, other)) return `Already used for ${title(which === "voice" ? "record" : "voice")}`;
  return null;
}

// From config.json (the parsed object): { record, voice } as shortcuts. Each one missing, unreadable or not
// allowed means its default; the two the same means both defaults.
export function fromConfig(config, platform) {
  const mac = platform === "mac";
  const keys = config && typeof config === "object" ? config.hotkeys : null;
  const one = (id) => {
    const v = keys && typeof keys === "object" ? keys[id] : null;
    const sc = typeof v === "string" ? parse(v) : null;
    return sc && !needsModifier(sc, mac) ? sc : parse(defaults[id]);
  };
  const record = one("record");
  const voice = one("voice");
  if (same(record, voice)) return { record: parse(defaults.record), voice: parse(defaults.voice) };
  return { record, voice };
}

// What goes into config.json "hotkeys": {"record": "Alt+C", "voice": "Alt+V"}.
export function toConfig(hotkeys) {
  return { record: text(hotkeys.record), voice: text(hotkeys.voice) };
}
