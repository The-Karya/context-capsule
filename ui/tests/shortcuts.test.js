// context-capsule — ui: tests for shortcuts.js (mirrors capture-rs/core/src/shortcuts.rs tests).
import { test, expect } from "bun:test";
import * as S from "../js/core/shortcuts.js";

const s = (t) => S.parse(t);

test("parse and write", () => {
  expect(s("Alt+C")).toEqual({ ctrl: false, alt: true, shift: false, cmd: false, key: "C" });
  expect(S.text(s(" alt + v "))).toBe("Alt+V");
  expect(S.text(s("Shift+Ctrl+k"))).toBe("Ctrl+Shift+K");
  expect(S.text(s("Cmd+Shift+Alt+Ctrl+F12"))).toBe("Ctrl+Alt+Shift+Cmd+F12");
  expect(S.text(s("Option+Command+9"))).toBe("Alt+Cmd+9");
  expect(S.text(s("F5"))).toBe("F5");
  expect(S.symbols(s("Ctrl+Alt+Shift+Cmd+K"))).toBe("⌃⌥⇧⌘K");
  expect(S.text(s("⌥+⌘+X"))).toBe("Alt+Cmd+X");
  for (const bad of ["", "Alt", "Alt+", "Alt+CC", "Alt+Alt+C", "Super+C", "Alt+F13", "Alt+F0", "Alt+F01", "C+Alt", "Alt+Tab", "Alt+é"]) {
    expect(S.parse(bad)).toBeNull();
  }
  for (const t of ["Alt+C", "Ctrl+Shift+5", "F1", "Ctrl+Alt+Shift+Cmd+F12"]) expect(S.text(s(t))).toBe(t);
});

test("format per platform", () => {
  expect(S.format("Alt+C", "mac")).toBe("⌥C");
  expect(S.format("Alt+C", "windows")).toBe("Alt+C");
  expect(S.format(s("ctrl+shift+f5"), "linux")).toBe("Ctrl+Shift+F5");
  expect(S.format("nonsense", "mac")).toBe("");
});

test("allowed, and the words", () => {
  expect(S.check("C", "Alt+V", "windows")).toBe("Add Ctrl or Alt");
  expect(S.check("Shift+C", "Alt+V", "mac")).toBe("Add ⌃, ⌥ or ⌘");
  expect(S.check("F7", "Alt+V", "linux")).toBeNull();
  expect(S.check("Shift+F7", "Alt+V", "linux")).toBeNull();
  expect(S.check("Ctrl+1", "Alt+V", "windows")).toBeNull();
  expect(S.check("Cmd+K", "Alt+V", "mac")).toBeNull();
  expect(S.check("Cmd+K", "Alt+V", "windows")).toBe("Add Ctrl or Alt");
  expect(S.check("Alt+V", "Alt+V", "windows", "record")).toBe("Already used for Speak a note");
  expect(S.check("Alt+C", "Alt+C", "mac", "voice")).toBe("Already used for Start or stop");
  expect(S.check("Ctrl+Alt+R", "Alt+V", "mac")).toBeNull();
  expect(S.TAKEN).toBe("Another app uses these keys");
});

test("config value", () => {
  const d = { record: s("Alt+C"), voice: s("Alt+V") };
  expect(S.fromConfig(null, "windows")).toEqual(d);
  expect(S.fromConfig({}, "windows")).toEqual(d);
  const h = S.fromConfig({ hotkeys: { record: "Ctrl+Shift+R", voice: "F9" } }, "windows");
  expect([S.text(h.record), S.text(h.voice)]).toEqual(["Ctrl+Shift+R", "F9"]);
  expect(S.fromConfig({ hotkeys: { record: "R", voice: "nonsense" } }, "linux")).toEqual(d);
  const v = S.fromConfig({ hotkeys: { voice: "Ctrl+N" } }, "linux");
  expect([S.text(v.record), S.text(v.voice)]).toEqual(["Alt+C", "Ctrl+N"]);
  expect(S.text(S.fromConfig({ hotkeys: { record: "Cmd+K" } }, "windows").record)).toBe("Alt+C");
  expect(S.text(S.fromConfig({ hotkeys: { record: "Cmd+K" } }, "mac").record)).toBe("Cmd+K");
  expect(S.fromConfig({ hotkeys: { record: "Alt+V" } }, "windows")).toEqual(d);
  expect(S.fromConfig({ hotkeys: "Alt+C" }, "windows")).toEqual(d);
  expect(S.toConfig(d)).toEqual(S.defaults);
});

test("key presses", () => {
  const e = (code, mods = {}) => ({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods });
  expect(S.fromKeyEvent(e("AltLeft", { altKey: true }), "mac")).toBeNull();
  expect(S.fromKeyEvent(e("ShiftLeft", { shiftKey: true }), "windows")).toBeNull();
  expect(S.text(S.fromKeyEvent(e("KeyK", { ctrlKey: true }), "windows"))).toBe("Ctrl+K");
  expect(S.text(S.fromKeyEvent(e("KeyK"), "windows"))).toBe("K");
  expect(S.text(S.fromKeyEvent(e("Digit7", { altKey: true, shiftKey: true }), "linux"))).toBe("Alt+Shift+7");
  expect(S.text(S.fromKeyEvent(e("F12"), "linux"))).toBe("F12");
  expect(S.fromKeyEvent(e("F13"), "linux")).toBeNull();
  expect(S.fromKeyEvent(e("Tab", { altKey: true }), "linux")).toBeNull();
  expect(S.format(S.fromKeyEvent(e("KeyC", { metaKey: true, altKey: true }), "mac"), "mac")).toBe("⌥⌘C");
  expect(S.fromKeyEvent(e("KeyC", { metaKey: true }), "windows")).toBeNull();
  expect(S.isCancel(e("Escape"))).toBe(true);
  expect(S.isCancel(e("Escape", { altKey: true }))).toBe(false);
});
