// context-capsule — mcp: tests for connecting AI apps (JSON / JSONC / TOML merging, .bak, idempotence, refusals)
import { beforeEach, describe, expect, test } from "bun:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { parse as parseToml } from "smol-toml";
import { CLIENTS, connect, disconnect, listClients, removeTomlTable, selfCommand, type Ctx } from "../src/clients.ts";

let home: string;
let ctx: Ctx;
let bin: string;
const self = () => ({ command: bin, args: ["serve"] });
const read = (p: string) => fs.readFileSync(p, "utf8");

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), "cc-clients-"));
  ctx = { os: "mac", home, env: {}, appDirs: [path.join(home, "Applications")] };
  bin = path.join(home, "bin", "context-capsule-mcp");
  fs.mkdirSync(path.dirname(bin), { recursive: true });
  fs.writeFileSync(bin, "#!/bin/sh\n");
});

const fileOf = (id: string, c: Ctx = ctx) => CLIENTS.find((d) => d.id === id)!.file!(c);
const clientOf = (id: string, c: Ctx = ctx) => listClients(c).find((x) => x.id === id)!;

describe("clients", () => {
  test("lists every app with the spec's fields; nothing installed in an empty home", () => {
    const list = listClients(ctx);
    expect(list.map((c) => c.id)).toEqual(["claude", "chatgpt", "codex", "cursor", "vscode", "windsurf", "lmstudio", "gemini"]);
    for (const c of list) {
      expect(Object.keys(c).sort()).toEqual(["connected", "how", "id", "installed", "name"]);
      expect(c.installed).toBe(false);
      expect(c.connected).toBe(false);
    }
    expect(list.find((c) => c.id === "claude")!.how).toBe("extension");
    expect(list.filter((c) => c.how === "config").length).toBe(7);
  });

  test("installed: the app or its config folder", () => {
    fs.mkdirSync(path.join(home, ".cursor"));
    fs.mkdirSync(path.join(home, "Applications", "ChatGPT.app"), { recursive: true });
    fs.mkdirSync(path.join(home, "Library", "Application Support", "Code", "User"), { recursive: true });
    const list = listClients(ctx);
    const on = list.filter((c) => c.installed).map((c) => c.id);
    expect(on).toEqual(["chatgpt", "cursor", "vscode"]);
  });

  test("paths per OS", () => {
    const win: Ctx = { os: "windows", home, env: { APPDATA: path.join(home, "Roaming"), LOCALAPPDATA: path.join(home, "Local") }, appDirs: [] };
    const linux: Ctx = { os: "linux", home, env: {}, appDirs: [] };
    expect(fileOf("vscode")).toBe(path.join(home, "Library", "Application Support", "Code", "User", "mcp.json"));
    expect(fileOf("vscode", win)).toBe(path.join(home, "Roaming", "Code", "User", "mcp.json"));
    expect(fileOf("vscode", linux)).toBe(path.join(home, ".config", "Code", "User", "mcp.json"));
    expect(fileOf("vscode", { ...linux, env: { XDG_CONFIG_HOME: path.join(home, "xdg") } })).toBe(path.join(home, "xdg", "Code", "User", "mcp.json"));
    expect(fileOf("codex")).toBe(path.join(home, ".codex", "config.toml"));
    expect(fileOf("chatgpt")).toBe(fileOf("codex"));
    expect(fileOf("codex", { ...ctx, env: { CODEX_HOME: path.join(home, "ch") } })).toBe(path.join(home, "ch", "config.toml"));
    expect(fileOf("cursor", win)).toBe(path.join(home, ".cursor", "mcp.json"));
    expect(fileOf("windsurf")).toBe(path.join(home, ".codeium", "windsurf", "mcp_config.json"));
    expect(fileOf("lmstudio", win)).toBe(path.join(home, ".lmstudio", "mcp.json"));
    expect(fileOf("gemini", linux)).toBe(path.join(home, ".gemini", "settings.json"));
    // Windows: ChatGPT from the Microsoft Store
    fs.mkdirSync(path.join(home, "Local", "Packages", "OpenAI.ChatGPT-Desktop_abc"), { recursive: true });
    expect(clientOf("chatgpt", win).installed).toBe(true);
  });

  test("Claude from the Microsoft Store: installed and connected through its private folder", () => {
    const win: Ctx = { os: "windows", home, env: { APPDATA: path.join(home, "Roaming"), LOCALAPPDATA: path.join(home, "Local") }, appDirs: [] };
    const store = path.join(home, "Local", "Packages", "Claude_pzs8sxrjxfjjc", "LocalCache", "Roaming", "Claude");
    fs.mkdirSync(path.join(home, "Local", "Packages", "Microsoft.Paint_8wekyb3d8bbwe"), { recursive: true });
    fs.mkdirSync(store, { recursive: true });
    expect(clientOf("claude", win)).toMatchObject({ installed: true, connected: false });
    fs.mkdirSync(path.join(store, "Claude Extensions", "local.mcpb.karya.context-capsule"), { recursive: true });
    expect(clientOf("claude", win)).toMatchObject({ installed: true, connected: true });
    expect(fs.existsSync(path.join(home, "Roaming", "Claude"))).toBe(false);
  });

  test("Claude: the extension, never a settings file", () => {
    const r = connect("claude", { ctx, self: self() });
    expect(r).toMatchObject({ ok: false, how: "extension" });
    expect(disconnect("claude", { ctx })).toMatchObject({ ok: false, how: "extension" });
    const dir = path.join(home, "Library", "Application Support", "Claude", "Claude Extensions", "local.mcpb.karya.context-capsule");
    fs.mkdirSync(dir, { recursive: true });
    expect(clientOf("claude")).toMatchObject({ installed: true, connected: true });
  });

  test("unknown and missing apps are refused plainly", () => {
    expect(connect("nope", { ctx, self: self() }).ok).toBe(false);
    const r = connect("cursor", { ctx, self: self() });
    expect(r.ok).toBe(false);
    expect(r.message).toContain("isn't on this computer");
    expect(fs.existsSync(fileOf("cursor"))).toBe(false);
  });
});

describe("JSON settings", () => {
  test("Cursor: creates the file, then merges next to other servers and keeps a .bak", () => {
    fs.mkdirSync(path.join(home, ".cursor"));
    const file = fileOf("cursor");
    expect(connect("cursor", { ctx, self: self() }).ok).toBe(true);
    expect(JSON.parse(read(file))).toEqual({ mcpServers: { "context-capsule": { command: bin, args: ["serve"] } } });
    expect(fs.existsSync(`${file}.bak`)).toBe(false); // nothing to back up

    const other = { mcpServers: { github: { command: "npx", args: ["-y", "gh-mcp"], env: { T: "x" } } }, theme: "dark" };
    fs.writeFileSync(file, JSON.stringify(other, null, 4));
    const r = connect("cursor", { ctx, self: self() });
    expect(r.ok).toBe(true);
    expect(r.message).toContain("Restart Cursor");
    const after = JSON.parse(read(file));
    expect(after.mcpServers.github).toEqual(other.mcpServers.github);
    expect(after.theme).toBe("dark");
    expect(after.mcpServers["context-capsule"]).toEqual({ command: bin, args: ["serve"] });
    expect(read(file)).toContain('    "mcpServers"'); // keeps the file's 4-space indent
    expect(JSON.parse(read(`${file}.bak`))).toEqual(other);
    expect(clientOf("cursor")).toMatchObject({ installed: true, connected: true });
  });

  test("connect and disconnect are idempotent", () => {
    fs.mkdirSync(path.join(home, ".codeium", "windsurf"), { recursive: true });
    const file = fileOf("windsurf");
    fs.writeFileSync(file, '{"mcpServers": {"a": {"command": "a"}}}\n');
    expect(connect("windsurf", { ctx, self: self() }).ok).toBe(true);
    const once = read(file);
    fs.rmSync(`${file}.bak`);
    const again = connect("windsurf", { ctx, self: self() });
    expect(again).toEqual({ ok: true, message: "Context Capsule is already connected to Windsurf." });
    expect(read(file)).toBe(once);
    expect(fs.existsSync(`${file}.bak`)).toBe(false); // nothing written

    expect(disconnect("windsurf", { ctx }).message).toBe("Disconnected from Windsurf.");
    expect(JSON.parse(read(file))).toEqual({ mcpServers: { a: { command: "a" } } });
    const gone = read(file);
    expect(disconnect("windsurf", { ctx }).message).toContain("wasn't connected");
    expect(read(file)).toBe(gone);
  });

  test("a moved binary is re-pointed; a missing binary isn't 'connected'", () => {
    fs.mkdirSync(path.join(home, ".lmstudio"));
    const file = fileOf("lmstudio");
    fs.writeFileSync(file, JSON.stringify({ mcpServers: { "context-capsule": { command: path.join(home, "old", "gone"), args: ["serve"] } } }));
    expect(clientOf("lmstudio").connected).toBe(false);
    expect(connect("lmstudio", { ctx, self: self() }).message).toContain("Connected");
    expect(clientOf("lmstudio").connected).toBe(true);
    // from source: node + an existing script
    const script = path.join(home, "dist", "index.js");
    fs.mkdirSync(path.dirname(script));
    fs.writeFileSync(script, "");
    connect("lmstudio", { ctx, self: { command: "node", args: [script, "serve"] } });
    expect(clientOf("lmstudio").connected).toBe(true);
    fs.rmSync(script);
    expect(clientOf("lmstudio").connected).toBe(false);
  });

  test("VS Code: servers with type stdio, comments and trailing commas kept", () => {
    const dir = path.join(home, "Library", "Application Support", "Code", "User");
    fs.mkdirSync(dir, { recursive: true });
    const file = path.join(dir, "mcp.json");
    const src = `{
  // my servers
  "servers": {
    "playwright": { "type": "stdio", "command": "npx", "args": ["-y", "@playwright/mcp@latest"] }, // browser
  },
  "inputs": [],
}
`;
    fs.writeFileSync(file, src);
    expect(connect("vscode", { ctx, self: self() }).ok).toBe(true);
    const out = read(file);
    expect(out).toContain("// my servers");
    expect(out).toContain("// browser");
    expect(out).toContain('"inputs": []');
    expect(out).toContain('"type": "stdio"');
    expect(out).toContain(JSON.stringify(bin));
    expect(read(`${file}.bak`)).toBe(src);
    expect(clientOf("vscode").connected).toBe(true);
    expect(disconnect("vscode", { ctx }).ok).toBe(true);
    expect(read(file)).toContain("playwright");
    expect(read(file)).not.toContain("context-capsule");
    expect(read(file)).toContain("// my servers");
  });

  test("Gemini CLI: other settings stay", () => {
    fs.mkdirSync(path.join(home, ".gemini"));
    const file = fileOf("gemini");
    fs.writeFileSync(file, JSON.stringify({ theme: "GitHub", selectedAuthType: "oauth" }, null, 2));
    expect(connect("gemini", { ctx, self: self() }).ok).toBe(true);
    expect(JSON.parse(read(file))).toEqual({
      theme: "GitHub",
      selectedAuthType: "oauth",
      mcpServers: { "context-capsule": { command: bin, args: ["serve"] } },
    });
  });

  test("never touches a file it can't read", () => {
    fs.mkdirSync(path.join(home, ".cursor"));
    const file = fileOf("cursor");
    for (const broken of ['{"mcpServers": {', "[1, 2]", '{"mcpServers": "nope"}', "not json at all"]) {
      fs.writeFileSync(file, broken);
      for (const r of [connect("cursor", { ctx, self: self() }), disconnect("cursor", { ctx })]) {
        expect(r.ok).toBe(false);
        expect(r.message).toContain("nothing was changed");
      }
      expect(read(file)).toBe(broken);
      expect(fs.existsSync(`${file}.bak`)).toBe(false);
      expect(clientOf("cursor").connected).toBe(false);
    }
  });

  test("an empty file counts as no settings yet", () => {
    fs.mkdirSync(path.join(home, ".cursor"));
    fs.writeFileSync(fileOf("cursor"), "  \n");
    expect(connect("cursor", { ctx, self: self() }).ok).toBe(true);
    expect(JSON.parse(read(fileOf("cursor"))).mcpServers["context-capsule"].command).toBe(bin);
  });
});

describe("TOML settings (ChatGPT and Codex)", () => {
  const original = `# my Codex settings
model = "gpt-5"

[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]

[mcp_servers.context7.env]
MY_ENV_VAR = "x" # keep me

# the docs server
[mcp_servers.docs]
url = "https://example.com/mcp"
`;

  test("adds [mcp_servers.context-capsule], keeps everything else and its comments", () => {
    fs.mkdirSync(path.join(home, ".codex"));
    const file = fileOf("codex");
    fs.writeFileSync(file, original);
    const r = connect("codex", { ctx, self: { command: "C:\\Program Files\\Context Capsule\\context-capsule-mcp.exe", args: ["serve"] } });
    expect(r.ok).toBe(true);
    const out = read(file);
    expect(out.startsWith(original)).toBe(true);
    const data = parseToml(out) as { mcp_servers: Record<string, { command: string; args: string[] }> };
    expect(data.mcp_servers["context-capsule"]).toEqual({ command: "C:\\Program Files\\Context Capsule\\context-capsule-mcp.exe", args: ["serve"] });
    expect(read(`${file}.bak`)).toBe(original);
  });

  test("ChatGPT and Codex share it; connect, re-connect, disconnect", () => {
    fs.mkdirSync(path.join(home, "Applications", "ChatGPT.app"), { recursive: true });
    fs.mkdirSync(path.join(home, ".codex"));
    const file = fileOf("chatgpt");
    fs.writeFileSync(file, original);
    expect(connect("chatgpt", { ctx, self: self() }).message).toContain("Restart ChatGPT");
    expect(clientOf("chatgpt").connected).toBe(true);
    expect(clientOf("codex").connected).toBe(true);
    const once = read(file);
    expect(connect("codex", { ctx, self: self() }).message).toContain("already connected");
    expect(read(file)).toBe(once);

    // a new place for the binary replaces the entry instead of adding a second one
    const bin2 = path.join(home, "bin", "new");
    fs.writeFileSync(bin2, "");
    expect(connect("codex", { ctx, self: { command: bin2, args: ["serve"] } }).ok).toBe(true);
    expect(read(file).match(/\[mcp_servers\.context-capsule\]/g)!.length).toBe(1);
    expect((parseToml(read(file)) as any).mcp_servers["context-capsule"].command).toBe(bin2); // TOML escapes \ on Windows

    expect(disconnect("chatgpt", { ctx }).ok).toBe(true);
    expect(read(file)).toBe(original);
    expect(disconnect("codex", { ctx }).message).toContain("wasn't connected");
  });

  test("our table in the middle: comments that belong to the next table stay", () => {
    const mid = `[mcp_servers.context-capsule]
command = "/old"
args = ["serve"]

[mcp_servers.context-capsule.env]
A = "1"

# the docs server
[mcp_servers.docs]
url = "https://example.com/mcp"
`;
    expect(removeTomlTable(mid)).toBe(`# the docs server\n[mcp_servers.docs]\nurl = "https://example.com/mcp"\n`);
    expect(removeTomlTable(`a = 1\n\n[mcp_servers."context-capsule"]\ncommand = "x"\n`)).toBe("a = 1\n");
  });

  test("never touches a TOML file it can't read, or can't edit safely", () => {
    fs.mkdirSync(path.join(home, ".codex"));
    const file = fileOf("codex");
    for (const broken of ['model = "gpt-5\n', "[mcp_servers\ncommand = 1", 'mcp_servers = "nope"\n', 'mcp_servers = { context-capsule = { command = "/x" } }\n']) {
      fs.writeFileSync(file, broken);
      const r = connect("codex", { ctx, self: self() });
      expect(r.ok).toBe(false);
      expect(r.message).toContain("nothing was changed");
      expect(read(file)).toBe(broken);
      expect(fs.existsSync(`${file}.bak`)).toBe(false);
    }
  });

  test("Windows line endings stay Windows line endings", () => {
    fs.mkdirSync(path.join(home, ".codex"));
    const file = fileOf("codex");
    const crlf = original.replace(/\n/g, "\r\n");
    fs.writeFileSync(file, crlf);
    expect(connect("codex", { ctx, self: self() }).ok).toBe(true);
    expect(read(file).startsWith(crlf)).toBe(true);
    expect(read(file).replace(/\r\n/g, "")).not.toContain("\n");
  });
});

describe("how an app starts us", () => {
  test("from source: node with the absolute dist/index.js, then serve", () => {
    const s = selfCommand();
    expect(path.isAbsolute(s.args[0]!)).toBe(true);
    expect(s.args[0]!.endsWith(path.join("dist", "index.js"))).toBe(true);
    expect(s.args[1]).toBe("serve");
    expect(path.basename(s.command)).toMatch(/^node(\.exe)?$/);
  });
});
