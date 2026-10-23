// context-capsule — ui: tests for the app-icon cache (bridge `apps.icons`): one call per moment, one ask per app,
// misses and errors keep the glyph.

import { expect, test } from "bun:test";
import { iconKey, makeIconCache } from "../js/core/appicons.js";

const PNG = "data:image/png;base64,iVBORw0KGgo=";

test("the key is the app's id, else its name", () => {
  expect(iconKey("Safari", "com.apple.Safari")).toBe("com.apple.Safari");
  expect(iconKey("Notes", "")).toBe("Notes");
  expect(iconKey(" Notes ", undefined)).toBe("Notes");
  expect(iconKey("", "")).toBe("");
});

test("apps drawn together are asked in one call, each once", async () => {
  const calls = [];
  const cache = makeIconCache(async (apps) => {
    calls.push(apps);
    return { "chrome.exe": PNG, Notes: null };
  });
  const got = [];
  expect(cache.want("Chrome", "chrome.exe", (u) => got.push(["a", u]))).toBeUndefined();
  expect(cache.want("Chrome", "chrome.exe", (u) => got.push(["b", u]))).toBeUndefined();
  expect(cache.want("Notes", "", (u) => got.push(["c", u]))).toBeUndefined();
  expect(cache.want("", "")).toBeNull();
  await Bun.sleep(0);
  expect(calls).toEqual([[{ app: "Chrome", bundle: "chrome.exe" }, { app: "Notes", bundle: "" }]]);
  expect(got).toEqual([["a", PNG], ["b", PNG], ["c", null]]);
  // Known now: answered straight away, no new call.
  expect(cache.want("Google Chrome", "chrome.exe")).toBe(PNG);
  expect(cache.want("Notes", "")).toBeNull();
  await Bun.sleep(0);
  expect(calls.length).toBe(1);
});

test("an app asked while a call is out waits for that call", async () => {
  let release;
  const calls = [];
  const cache = makeIconCache((apps) => {
    calls.push(apps);
    return new Promise((r) => (release = () => r({ Figma: PNG })));
  });
  const got = [];
  cache.want("Figma", "", (u) => got.push(u));
  await Bun.sleep(0);
  cache.want("Figma", "", (u) => got.push(u));
  await Bun.sleep(0);
  release();
  await Bun.sleep(0);
  expect(calls.length).toBe(1);
  expect(got).toEqual([PNG, PNG]);
});

test("a host without the method, or odd answers, keep the glyph", async () => {
  const failing = makeIconCache(async () => {
    throw new Error("unknown method apps.icons");
  });
  const got = [];
  failing.want("Slack", "com.tinyspeck.slackmacgap", (u) => got.push(u));
  await Bun.sleep(0);
  await Bun.sleep(0);
  expect(got).toEqual([null]);
  expect(failing.want("Slack", "com.tinyspeck.slackmacgap")).toBeNull();

  const odd = makeIconCache(async () => ({ a: "javascript:alert(1)", b: 42 }));
  const seen = [];
  odd.want("a", "", (u) => seen.push(u));
  odd.want("b", "", (u) => seen.push(u));
  await Bun.sleep(0);
  await Bun.sleep(0);
  expect(seen).toEqual([null, null]);
});
