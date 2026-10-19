// context-capsule — mcp: tests for per-OS words and the data folder rule
import { afterEach, describe, expect, test } from "bun:test";
import path from "node:path";
import { capsuleHome } from "../src/paths.ts";
import { currentOS, words } from "../src/platform.ts";

const saved = { ...process.env };
afterEach(() => {
  process.env = { ...saved };
});

describe("platform words", () => {
  test("maps node platforms", () => {
    expect(currentOS("darwin")).toBe("mac");
    expect(currentOS("win32")).toBe("windows");
    expect(currentOS("linux")).toBe("linux");
  });

  test("each OS names its own hotkey and how to open the app", () => {
    expect(words("mac").recordKeyShort).toBe("⌥⇧C");
    expect(words("mac").openApp).toContain("open -a");
    expect(words("windows").recordKeyShort).toBe("Ctrl+Alt+Shift+C");
    expect(words("windows").openApp).not.toContain("open -a");
    expect(words("linux").openApp).toContain("context-capsule");
    expect(words("linux").picker).not.toContain("macOS");
  });
});

describe("data folder", () => {
  test("CONTEXT_CAPSULE_HOME wins everywhere", () => {
    process.env.CONTEXT_CAPSULE_HOME = "/tmp/cc-home";
    process.env.XDG_DATA_HOME = "/tmp/xdg";
    expect(capsuleHome("linux")).toBe(path.resolve("/tmp/cc-home"));
  });

  test("Linux uses XDG_DATA_HOME when set, else ~/.context-capsule", () => {
    delete process.env.CONTEXT_CAPSULE_HOME;
    process.env.HOME = "/home/saachi";
    process.env.XDG_DATA_HOME = "/home/saachi/.local/share";
    const home = path.join("/home/saachi", ".context-capsule");
    expect(capsuleHome("linux")).toBe(path.join("/home/saachi/.local/share", "context-capsule"));
    expect(capsuleHome("darwin")).toBe(home);
    delete process.env.XDG_DATA_HOME;
    expect(capsuleHome("linux")).toBe(home);
    process.env.XDG_DATA_HOME = "relative/path";
    expect(capsuleHome("linux")).toBe(home);
  });
});
