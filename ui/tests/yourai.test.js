// context-capsule — ui: tests for yourai.js (mirrors capture-rs/core/src/anyai.rs tests).
import { test, expect } from "bun:test";
import * as A from "../js/core/yourai.js";

const client = (id, installed, connected) => ({ id, name: id.toUpperCase(), installed, connected, how: "config" });
const claude = (installed, connected) => ({ id: "claude", name: "Claude", installed, connected, how: "extension" });

test("reads answers", () => {
  const list = A.parseClients(`[{"id":"claude","name":"Claude","installed":true,"connected":false,"how":"extension"},
    {"id":"cursor","name":"Cursor","installed":false,"connected":false,"how":"config","extra":1},{"name":"no id"}]`);
  expect(list.length).toBe(2);
  expect(list[0].installed && list[0].how === "extension").toBe(true);
  expect(A.parseClients('{"clients":[{"id":"x"}]}')[0]).toEqual({ id: "x", name: "x", installed: false, connected: false, how: "config" });
  expect(A.parseClients('bun says hi\n[{"id":"a"}]\n').length).toBe(1);
  expect(() => A.parseClients("nope")).toThrow("bad");
  expect(() => A.parseClients('{"x":1}')).toThrow("bad");
  expect(A.parseConnect('{"ok":false,"how":"extension","message":"m"}')).toEqual({ ok: false, message: "m", how: "extension" });
  expect(A.parseDay('{"text":"héllo","chars":5}')).toEqual({ text: "héllo", chars: 5 });
  expect(A.parseDay('{"text":"héllo"}').chars).toBe(5);
  expect(() => A.parseDay('{"error":"\\"x\\" is not a date."}')).toThrow('"x" is not a date.');
  const s = A.parseSave('{"saved":[{"title":"","path":"/p"}],"refused":[{"title":"B","why":"no Next steps"}]}');
  expect(s.saved[0]).toEqual({ title: "Untitled", path: "/p" });
  expect(s.refused[0].why).toBe("no Next steps");
  expect(A.parseSave('{"saved":[]}')).toEqual({ saved: [], refused: [] });
  expect(() => A.parseSave('{"error":"boom"}')).toThrow("boom");
});

test("order and choice", () => {
  const list = A.sorted([client("zed", true, false), client("cursor", false, false), claude(true, false), client("codex", true, true)]);
  expect(list.map((c) => c.id)).toEqual(["claude", "codex", "zed", "cursor"]);
  expect(A.resolve(null, [])).toBe("any");
  expect(A.resolve("cursor", [])).toBe("cursor");
  expect(A.resolve("gone", list)).toBe("codex");
  expect(A.resolve(" chatgpt-web ", list)).toBe("chatgpt-web");
  expect(A.resolve(null, [claude(true, true), client("codex", true, true)])).toBe("claude");
  expect(A.resolve(null, [client("a", true, false)])).toBe("any");

  expect(A.kindOf("claude")).toEqual({ type: "claude", id: "claude" });
  expect(A.kindOf("").type).toBe("other");
  expect(A.kindOf("gemini-web")).toEqual({ type: "website", id: "gemini-web", url: "https://gemini.google.com" });
  expect(A.usesCopy(A.kindOf("gemini-web")) && A.usesCopy(A.kindOf("any"))).toBe(true);
  expect(A.usesCopy(A.kindOf("cursor")) || A.usesCopy(A.kindOf("claude"))).toBe(false);
  expect(A.choiceName(A.kindOf("chatgpt-web"), [])).toBe("ChatGPT");
  expect(A.choiceName(A.kindOf("codex"), list)).toBe("CODEX");
  expect(A.choiceName(A.kindOf("vscode"), [])).toBe("vscode");
  expect(A.choiceName(A.kindOf("any"), [])).toBe("your AI");
  expect(A.homeLine(A.kindOf("any"), "your AI")).toBe("Your AI: any AI");
  expect(A.homeLine(A.kindOf("cursor"), "Cursor")).toBe("Your AI: Cursor");

  expect(A.firstConnectedBecomesChoice(null, [client("a", true, false)])).toBe(true);
  expect(A.firstConnectedBecomesChoice("any", [])).toBe(false);
  expect(A.firstConnectedBecomesChoice(null, [client("a", true, true)])).toBe(false);

  expect(A.choices(list, "cursor").map((c) => c.label)).toEqual(
    ["Claude (not connected)", "CODEX", "ZED (not connected)", "CURSOR (not connected)", "ChatGPT (website)", "Gemini (website)", "Any other AI"]);
  expect(A.choices([], "vscode")[0]).toEqual({ id: "vscode", label: "vscode" });
});

test("words", () => {
  expect(A.makeLine(A.kindOf("claude"), "Claude", null, "Enter")).toBe("Claude opens with the request typed in. Press Enter.");
  expect(A.makeLine(A.kindOf("claude"), "Claude")).toBe("Claude opens with the request typed in. Press Return.");
  expect(A.makeLine(A.kindOf("cursor"), "Cursor")).toBe("Cursor opens. Paste the request in a new chat.");
  expect(A.makeLine(A.kindOf("any"), "your AI")).toBe("Copy your day, paste it into your AI, then save its reply here.");
  expect(A.makeLine(A.kindOf("any"), "your AI", A.pasteLine)).toBe(A.pasteLine);
  expect(A.settingsLine(A.kindOf("gemini-web"), "Gemini")).toBe("Make today's capsules copies your day to paste into Gemini, then you save its reply.");
  expect(A.settingsLine(A.kindOf("claude"), "Claude")).toBe("Make today's capsules opens Claude with the request.");
  expect(A.helpMakeKeys(A.kindOf("any"))).toBe("Copy my day");
  expect(A.helpMakeKeys(A.kindOf("claude"))).toBe("in your AI");
  expect(A.connectFirst("Cursor", "to read your day", "click Make today's capsules again").message)
    .toBe("Cursor needs Context Capsule to read your day. Click Connect. After that, click Make today's capsules again.");
  expect(A.copiedFor("Cursor", false)).toBe("Copied. Open Cursor and paste it in a new chat.");
  expect(A.copiedFor("Cursor", true)).toBe("Copied. Paste it in a new chat in Cursor.");
  expect(A.failureMessage("  ")).toBe("Something went wrong. Please try again.");
  expect(A.failureMessage("missing")).toContain("download Context Capsule again");
  expect(A.failureMessage("It broke.")).toBe("It broke.");
});

test("save summary", () => {
  const none = { title: "Nothing was saved", detail: "No capsules found in what you copied. Copy your AI's whole answer, then try again." };
  expect(A.saveSummary({ saved: [], refused: [] })).toEqual(none);
  const marker = { title: "Untitled", why: 'No capsule found in the pasted text. Copy the AI\'s whole answer: each capsule is between "=== CAPSULE ===" and "=== END ===".' };
  expect(A.saveSummary({ saved: [], refused: [marker] })).toEqual(none);
  expect(JSON.stringify(A.saveSummary({ saved: [], refused: [marker] }))).not.toContain("===");
  const r = { saved: [{ title: "Website", path: "/a" }], refused: [{ title: "Shop", why: "no Next steps" }, { title: "X", why: "" }] };
  expect(A.saveSummary(r)).toEqual({ title: "Saved 1 capsule", detail: "✓ Website\n\nNot saved:\n• Shop: no Next steps\n• X" });
  expect(A.saveSummary({ saved: [], refused: r.refused }).title).toBe("Nothing was saved");
  expect(A.saveSummary({ saved: [r.saved[0], r.saved[0]], refused: [] }).title).toBe("Saved 2 capsules");
});

test("Codex and ChatGPT share one connection, and both rows say so", () => {
  const codex = { id: "codex", name: "Codex", installed: true, connected: true, how: "config" };
  const chatgpt = { id: "chatgpt", name: "ChatGPT", installed: true, connected: true, how: "config" };
  const cursor = { id: "cursor", name: "Cursor", installed: true, connected: true, how: "config" };
  const all = [chatgpt, codex, cursor];
  expect(A.sharesWith(chatgpt, all)).toEqual([codex]);
  expect(A.sharesWith(codex, all)).toEqual([chatgpt]);
  expect(A.sharesWith(cursor, all)).toEqual([]);
  // the other one isn't installed: nothing to explain
  expect(A.sharesWith(codex, [{ ...chatgpt, installed: false }, codex])).toEqual([]);
  expect(A.sharesNote([codex])).toBe("Shares its connection with Codex");
  expect(A.sharesNote([chatgpt])).toBe("Shares its connection with ChatGPT");
  expect(A.disconnectBoth(chatgpt, [codex])).toBe("Disconnect ChatGPT and Codex…");
  expect(A.disconnectBothTitle(codex, [chatgpt])).toBe("Disconnect Codex and ChatGPT?");
  expect(A.disconnectBothBody(chatgpt, [codex])).toBe(
    "ChatGPT and Codex share one connection, so both will be disconnected. You can connect them again here anytime.",
  );
  expect(A.disconnectBothButton([codex])).toBe("Disconnect Both");
});
