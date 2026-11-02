// context-capsule — mcp: checks the compiled program, the one people really run, on the OS this runs on: connecting apps
// writes an entry that starts, an old broken entry is replaced, setup and remove work. Everything in a throwaway home.
// Usage: node mcp/ci/check-program.mjs <path to the compiled program>. Exits 1 if anything fails.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { parse as parseToml } from "smol-toml";

const program = fs.realpathSync(process.argv[2] ?? "");
const SERVER = "context-capsule";
const realHome = os.homedir();
const realFiles = [".codex/config.toml", ".cursor/mcp.json", ".gemini/settings.json"].map((f) => path.join(realHome, f));
const hashOf = (f) => (fs.existsSync(f) ? createHash("sha256").update(fs.readFileSync(f)).digest("hex") : null);
const realBefore = realFiles.map(hashOf);

let failed = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${!ok && detail ? `\n      ${detail}` : ""}`);
  if (!ok) failed++;
}

/** A new throwaway home with these folders in it, and the environment that points everything there. */
function newHome(...dirs) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "cc-check-"));
  for (const d of dirs) fs.mkdirSync(path.join(home, d), { recursive: true });
  const env = { ...process.env, HOME: home, USERPROFILE: home, APPDATA: path.join(home, "Roaming"), LOCALAPPDATA: path.join(home, "Local"),
    XDG_CONFIG_HOME: path.join(home, ".config"), XDG_DATA_HOME: path.join(home, ".data"), CONTEXT_CAPSULE_HOME: path.join(home, "data") };
  delete env.CODEX_HOME;
  return { home, env };
}
const run = ({ env }, ...args) => spawnSync(program, args, { env, encoding: "utf8", timeout: 60000 });
const json = (r) => { try { return JSON.parse(r.stdout); } catch { return null; } };
const client = (h, id) => json(run(h, "clients"))?.find((c) => c.id === id);
const samePath = (a, b) => { try { return fs.realpathSync(a).toLowerCase() === fs.realpathSync(b).toLowerCase(); } catch { return false; } };
const isProgram = (e) => !!e && samePath(e.command, program) && JSON.stringify(e.args) === '["serve"]';
const codexEntry = (h) => parseToml(fs.readFileSync(path.join(h.home, ".codex", "config.toml"), "utf8")).mcp_servers?.[SERVER];

/** Start an entry the way an AI app does, and ask it for its tools. */
async function toolsOf(entry, h, cwd) {
  const c = new Client({ name: "check-program", version: "1" });
  await c.connect(new StdioClientTransport({ command: entry.command, args: entry.args, env: h.env, cwd, stderr: "ignore" }));
  try {
    return (await c.listTools()).tools.map((t) => t.name);
  } finally {
    await c.close();
  }
}

console.log(`${process.platform} ${process.arch}: ${program}\n`);

const v = run(newHome(), "--version");
check("--version", v.status === 0 && /\d+\.\d+\.\d+/.test(v.stdout), v.stderr || v.error?.message);

// Connect: the entry is the program itself, on every kind of settings file
{
  const h = newHome(".codex", ".cursor");
  const before = "# my settings\n[other]\nx = 1\n";
  fs.writeFileSync(path.join(h.home, ".codex", "config.toml"), before);
  const r = run(h, "connect", "codex");
  check("connect codex answers ok", r.status === 0 && json(r)?.ok === true, r.stdout + r.stderr);
  const e = codexEntry(h);
  check("the Codex entry is the program itself, not node or a path inside it", isProgram(e), JSON.stringify(e));
  check("the app then says connected", client(h, "codex")?.connected === true, JSON.stringify(client(h, "codex")));
  check("the other settings stay", fs.readFileSync(path.join(h.home, ".codex", "config.toml"), "utf8").replace(/\r\n/g, "\n").startsWith(before));
  check("a .bak is kept", fs.existsSync(path.join(h.home, ".codex", "config.toml.bak")));
  run(h, "connect", "cursor");
  const cursor = JSON.parse(fs.readFileSync(path.join(h.home, ".cursor", "mcp.json"), "utf8")).mcpServers?.[SERVER];
  check("the Cursor entry is the program itself", isProgram(cursor), JSON.stringify(cursor));
  try {
    const tools = e ? await toolsOf(e, h) : [];
    check("the entry really starts and lists its tools", tools.length >= 21 && tools.includes("prepare_day") && tools.includes("compare_capsules"), `${tools.length} tools`);
  } catch (err) {
    check("the entry really starts and lists its tools", false, err.message);
  }
  const off = run(h, "disconnect", "codex");
  check("disconnect takes it out", off.status === 0 && codexEntry(h) === undefined && client(h, "codex")?.connected === false, off.stdout);
}

// The entry 0.3.0 wrote on Windows
{
  const h = newHome(".codex");
  fs.writeFileSync(path.join(h.home, ".codex", "config.toml"), '[mcp_servers.context-capsule]\ncommand = "node"\nargs = ["B:\\\\~BUN\\\\root\\\\index.js", "serve"]\n');
  check("an old broken entry reads as not connected", client(h, "codex")?.connected === false, JSON.stringify(client(h, "codex")));
  run(h, "connect", "codex");
  check("Connect replaces it", isProgram(codexEntry(h)) && client(h, "codex")?.connected === true, JSON.stringify(codexEntry(h)));
}

// setup and remove
{
  const h = newHome(".cursor");
  const on = run(h, "setup", "cursor");
  check("setup connects", on.status === 0 && on.stdout.includes("Connected to Cursor"), on.stdout + on.stderr);
  const off = run(h, "remove", "cursor");
  check("remove takes it out", off.status === 0 && off.stdout.includes("Disconnected from Cursor"), off.stdout + off.stderr);
  const none = run(newHome(), "setup", "gemini");
  check("setup says so when an app isn't here", none.status === 1 && none.stdout.includes("✗"), none.stdout);
}

// Claude from the Microsoft Store, with the extension in its private folder (Windows only)
if (process.platform === "win32") {
  const store = path.join("Local", "Packages", "Claude_pzs8sxrjxfjjc", "LocalCache", "Roaming", "Claude");
  const h = newHome(path.join(store, "Claude Extensions", "local.mcpb.karya.context-capsule"));
  const c = client(h, "claude");
  check("Claude from the Store shows installed and connected", c?.installed === true && c?.connected === true, JSON.stringify(c));
}

// The pasting route, with nothing recorded and with a junk reply
{
  const h = newHome("data");
  const d = run(h, "day");
  check("day with nothing recorded answers plainly", json(d) !== null, d.stdout + d.stderr);
  const s = spawnSync(program, ["save"], { env: h.env, input: "not a capsule", encoding: "utf8", timeout: 60000 });
  check("save with a junk reply saves nothing", (json(s)?.saved ?? []).length === 0, s.stdout + s.stderr);
}

// The npx route, as the install buttons and README would run it, with the package unpacked here instead of from npm
const pack = process.env.CC_PACKAGE_TGZ;
if (pack) {
  const h = newHome();
  const work = path.join(h.home, "work");
  fs.mkdirSync(work);
  const shell = process.platform === "win32";
  const i = spawnSync("npm", ["install", "--no-audit", "--no-fund", pack], { cwd: work, env: h.env, encoding: "utf8", shell, timeout: 300000 });
  check("the npm package installs", i.status === 0, i.stderr);
  const entry = shell ? { command: "cmd", args: ["/c", "npx", "-y", "context-capsule-mcp", "serve"] } : { command: "npx", args: ["-y", "context-capsule-mcp", "serve"] };
  try {
    const tools = await toolsOf(entry, h, work);
    check(`the npx entry starts (${entry.command})`, tools.includes("prepare_day"), `${tools.length} tools`);
  } catch (err) {
    check(`the npx entry starts (${entry.command})`, false, err.message);
  }
}

check("the real settings on this computer didn't change", realFiles.every((f, n) => hashOf(f) === realBefore[n]));
console.log(failed ? `\n${failed} failed` : "\nall passed");
process.exit(failed ? 1 : 0);
