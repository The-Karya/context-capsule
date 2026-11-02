// context-capsule — mcp: the AI apps on this computer and their MCP settings files: which are installed, which
// already have Context Capsule, and connecting / disconnecting it. Edits merge into the app's own file (JSON with
// comments, or TOML), keep every other entry, keep a .bak, and never touch a file that can't be read.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import * as jsoncNs from "jsonc-parser";
import { parse as parseToml } from "smol-toml";
import { writeFileAtomic } from "./fsutil.js";
import { homeDir } from "./paths.js";
import { currentOS, type OS } from "./platform.js";

// jsonc-parser is UMD for Node (everything on the default export) and ESM for bundlers (named exports only)
const jsonc: typeof jsoncNs = typeof jsoncNs.modify === "function" ? jsoncNs : (Reflect.get(jsoncNs, "default") as typeof jsoncNs);

/** Our entry's name in every app's settings. */
export const ENTRY = "context-capsule";

export type How = "extension" | "config";

export interface ClientInfo {
  id: string;
  name: string;
  installed: boolean;
  connected: boolean;
  how: How;
}

export interface ConnectResult {
  ok: boolean;
  message: string;
  how?: How;
}

/** Where to look: the OS, the home folder, the environment, and where apps are installed (Mac). */
export interface Ctx {
  os: OS;
  home: string;
  env: Record<string, string | undefined>;
  /** Mac: folders holding .app bundles. */
  appDirs: string[];
}

export function defaultCtx(): Ctx {
  const home = homeDir();
  return { os: currentOS(), home, env: process.env, appDirs: ["/Applications", path.join(home, "Applications")] };
}

/** How an app should start this program: the command and its arguments. */
export interface SelfCommand {
  command: string;
  args: string[];
}

type Format = { kind: "json"; key: "mcpServers" | "servers"; vscode?: boolean } | { kind: "toml" };

interface ClientDef {
  id: string;
  name: string;
  how: How;
  installed: (c: Ctx) => boolean;
  /** The settings file Context Capsule is added to (none for the Claude app: it uses the extension). */
  file?: (c: Ctx) => string;
  format?: Format;
}

const exists = (p: string) => {
  try {
    return fs.existsSync(p);
  } catch {
    return false;
  }
};
const macApp = (c: Ctx, name: string) => c.os === "mac" && c.appDirs.some((d) => exists(path.join(d, `${name}.app`)));
const appData = (c: Ctx) => c.env.APPDATA?.trim() || path.join(c.home, "AppData", "Roaming");
const localAppData = (c: Ctx) => c.env.LOCALAPPDATA?.trim() || path.join(c.home, "AppData", "Local");
const xdgConfig = (c: Ctx) => {
  const x = c.env.XDG_CONFIG_HOME?.trim();
  return x && path.isAbsolute(x) ? x : path.join(c.home, ".config");
};

/** The Claude desktop app's own folder. */
export function claudeDir(c: Ctx): string {
  if (c.os === "mac") return path.join(c.home, "Library", "Application Support", "Claude");
  if (c.os === "windows") return path.join(appData(c), "Claude");
  return path.join(xdgConfig(c), "Claude");
}

/** Every folder Claude keeps its data in: claudeDir, and on Windows the Microsoft Store app's private copy
 *  (%LOCALAPPDATA%\Packages\<Claude…>\LocalCache\Roaming\Claude), where its extensions go. */
export function claudeDirs(c: Ctx): string[] {
  const dirs = [claudeDir(c)];
  if (c.os === "windows") {
    try {
      for (const n of fs.readdirSync(path.join(localAppData(c), "Packages"))) {
        if (n.split("_")[0]!.toLowerCase().includes("claude"))
          dirs.push(path.join(localAppData(c), "Packages", n, "LocalCache", "Roaming", "Claude"));
      }
    } catch {
      // no Packages folder
    }
  }
  return dirs;
}

/** VS Code's user folder (where its user-level mcp.json lives). */
export function vscodeUserDir(c: Ctx): string {
  if (c.os === "mac") return path.join(c.home, "Library", "Application Support", "Code", "User");
  if (c.os === "windows") return path.join(appData(c), "Code", "User");
  return path.join(xdgConfig(c), "Code", "User");
}

/** Codex and the ChatGPT desktop app share one config: $CODEX_HOME, else ~/.codex. */
export function codexHome(c: Ctx): string {
  const h = c.env.CODEX_HOME?.trim();
  return h && path.isAbsolute(h) ? h : path.join(c.home, ".codex");
}

function chatgptInstalled(c: Ctx): boolean {
  if (macApp(c, "ChatGPT")) return true;
  if (c.os === "windows") {
    // installed from the Microsoft Store (OpenAI.ChatGPT-Desktop_…), or by its own installer
    try {
      if (fs.readdirSync(path.join(localAppData(c), "Packages")).some((n) => n.startsWith("OpenAI.ChatGPT"))) return true;
    } catch {
      // no Packages folder
    }
    return exists(path.join(localAppData(c), "Programs", "ChatGPT"));
  }
  return false;
}

export const CLIENTS: ClientDef[] = [
  {
    id: "claude",
    name: "Claude",
    how: "extension",
    installed: (c) =>
      macApp(c, "Claude") || claudeDirs(c).some(exists) || (c.os === "windows" && exists(path.join(localAppData(c), "AnthropicClaude"))),
  },
  {
    id: "chatgpt",
    name: "ChatGPT",
    how: "config",
    installed: chatgptInstalled,
    file: (c) => path.join(codexHome(c), "config.toml"),
    format: { kind: "toml" },
  },
  {
    id: "codex",
    name: "Codex",
    how: "config",
    installed: (c) => exists(codexHome(c)) || macApp(c, "Codex"),
    file: (c) => path.join(codexHome(c), "config.toml"),
    format: { kind: "toml" },
  },
  {
    id: "cursor",
    name: "Cursor",
    how: "config",
    installed: (c) => exists(path.join(c.home, ".cursor")) || macApp(c, "Cursor"),
    file: (c) => path.join(c.home, ".cursor", "mcp.json"),
    format: { kind: "json", key: "mcpServers" },
  },
  {
    id: "vscode",
    name: "VS Code",
    how: "config",
    installed: (c) => exists(vscodeUserDir(c)) || macApp(c, "Visual Studio Code"),
    file: (c) => path.join(vscodeUserDir(c), "mcp.json"),
    format: { kind: "json", key: "servers", vscode: true },
  },
  {
    id: "windsurf",
    name: "Windsurf",
    how: "config",
    installed: (c) => exists(path.join(c.home, ".codeium", "windsurf")) || macApp(c, "Windsurf"),
    file: (c) => path.join(c.home, ".codeium", "windsurf", "mcp_config.json"),
    format: { kind: "json", key: "mcpServers" },
  },
  {
    id: "lmstudio",
    name: "LM Studio",
    how: "config",
    installed: (c) => exists(path.join(c.home, ".lmstudio")) || macApp(c, "LM Studio"),
    file: (c) => path.join(c.home, ".lmstudio", "mcp.json"),
    format: { kind: "json", key: "mcpServers" },
  },
  {
    id: "gemini",
    name: "Gemini CLI",
    how: "config",
    installed: (c) => exists(path.join(c.home, ".gemini")),
    file: (c) => path.join(c.home, ".gemini", "settings.json"),
    format: { kind: "json", key: "mcpServers" },
  },
];

// ---------- this program, as an app should start it ----------

/** Running as the self-contained binary built with `bun build --compile`? */
export function isCompiled(url: string = import.meta.url): boolean {
  // On Windows the address is file:///B:/%7EBUN/root/…, with the ~ encoded.
  return /\$bunfs|~BUN|%7EBUN/i.test(url);
}

function findOnPath(name: string, env: Record<string, string | undefined> = process.env): string | null {
  const exts = process.platform === "win32" ? [".exe", ".cmd", ""] : [""];
  for (const dir of (env.PATH ?? "").split(path.delimiter)) {
    if (!dir) continue;
    for (const ext of exts) {
      const p = path.join(dir, name + ext);
      if (exists(p)) return p;
    }
  }
  return null;
}

const PACKAGE = "context-capsule-mcp";

/** Run with npx or bunx: this copy sits in a cache that can go away, so an app must not point at it. */
export function isTemporaryRun(dir: string): boolean {
  return /[\\/]_npx[\\/]|[\\/]bunx-/.test(dir);
}

/** What an app starts to run the npm package. On Windows npx is a script, which apps can only start through cmd. */
export function npxCommand(platform: NodeJS.Platform = process.platform): SelfCommand {
  const npx = ["npx", "-y", PACKAGE, "serve"];
  return platform === "win32" ? { command: "cmd", args: ["/c", ...npx] } : { command: npx[0]!, args: npx.slice(1) };
}

/** The binary runs itself with `serve`; from source, `node <abs>/dist/index.js serve`. */
export function selfCommand(): SelfCommand {
  if (isCompiled()) return { command: process.execPath, args: ["serve"] };
  const dir = path.dirname(fileURLToPath(import.meta.url));
  if (isTemporaryRun(dir)) return npxCommand();
  const entry = path.basename(dir) === "src" ? path.join(dir, "..", "dist", "index.js") : path.join(dir, "index.js");
  const node = !process.versions.bun && /node(\.exe)?$/i.test(process.execPath) ? process.execPath : (findOnPath("node") ?? "node");
  return { command: node, args: [path.resolve(entry), "serve"] };
}

/** An entry is "connected" only if it starts something that exists: the binary, or node with an existing script. */
export function pointsAtExisting(entry: unknown): boolean {
  if (!entry || typeof entry !== "object") return false;
  const { command, args } = entry as { command?: unknown; args?: unknown };
  if (typeof command !== "string" || !command.trim()) return false;
  const list = Array.isArray(args) ? args.filter((a): a is string => typeof a === "string") : [];
  // npx finds the package itself: there's no path of ours to check
  const run = /^cmd(\.exe)?$/i.test(path.basename(command)) && /^\/c$/i.test(list[0] ?? "") ? list.slice(1) : [command, ...list];
  if (/^npx(\.cmd)?$/i.test(path.basename(run[0] ?? "")) && run.slice(1).some((a) => a === PACKAGE || a.startsWith(`${PACKAGE}@`))) return true;
  const isNode = /^node(\.exe)?$/i.test(path.basename(command));
  if (isNode) return !!list[0] && path.isAbsolute(list[0]) && exists(list[0]) && (!path.isAbsolute(command) || exists(command));
  return path.isAbsolute(command) && exists(command);
}

// ---------- reading an app's settings ----------

function readEntry(def: ClientDef, c: Ctx): unknown {
  const file = def.file!(c);
  if (!exists(file)) return undefined;
  const text = fs.readFileSync(file, "utf8");
  if (def.format!.kind === "toml") {
    try {
      const data = parseToml(text) as Record<string, unknown>;
      return (data.mcp_servers as Record<string, unknown> | undefined)?.[ENTRY];
    } catch {
      return undefined;
    }
  }
  const errors: jsoncNs.ParseError[] = [];
  const value = jsonc.parse(text, errors, { allowTrailingComma: true }) as Record<string, unknown> | undefined;
  if (errors.length || !value || typeof value !== "object") return undefined;
  const servers = value[def.format!.key];
  return servers && typeof servers === "object" ? (servers as Record<string, unknown>)[ENTRY] : undefined;
}

function claudeConnected(c: Ctx): boolean {
  return claudeDirs(c).some((dir) => claudeConnectedIn(dir));
}

function claudeConnectedIn(dir: string): boolean {
  try {
    if (fs.readdirSync(path.join(dir, "Claude Extensions")).some((n) => n.includes("context-capsule"))) return true;
  } catch {
    // no extensions folder
  }
  const errors: jsoncNs.ParseError[] = [];
  const file = path.join(dir, "claude_desktop_config.json");
  if (!exists(file)) return false;
  const cfg = jsonc.parse(fs.readFileSync(file, "utf8"), errors, { allowTrailingComma: true }) as
    | { mcpServers?: Record<string, unknown> }
    | undefined;
  return !errors.length && pointsAtExisting(cfg?.mcpServers?.[ENTRY]);
}

export function listClients(c: Ctx = defaultCtx()): ClientInfo[] {
  return CLIENTS.map((d) => {
    let connected = false;
    try {
      connected = d.how === "extension" ? claudeConnected(c) : pointsAtExisting(readEntry(d, c));
    } catch {
      connected = false;
    }
    let installed = false;
    try {
      installed = d.installed(c);
    } catch {
      installed = false;
    }
    return { id: d.id, name: d.name, installed, connected, how: d.how };
  });
}

// ---------- connecting ----------

function same(a: unknown, b: unknown): boolean {
  return JSON.stringify(sortKeys(a)) === JSON.stringify(sortKeys(b));
}
function sortKeys(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === "object")
    return Object.fromEntries(Object.entries(v as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)).map(([k, x]) => [k, sortKeys(x)]));
  return v;
}

const cantRead = (file: string) =>
  `Couldn't read ${file}, so nothing was changed. Fix or remove that file in the app's own settings, then try again.`;

function writeWithBackup(file: string, before: string | null, after: string): void {
  if (before !== null) fs.copyFileSync(file, `${file}.bak`);
  writeFileAtomic(file, after);
}

/** The indent a JSON file already uses (2 spaces when unknown). */
function indentOf(text: string): { insertSpaces: boolean; tabSize: number } {
  const m = /^([ \t]+)\S/m.exec(text);
  if (!m) return { insertSpaces: true, tabSize: 2 };
  if (m[1]!.startsWith("\t")) return { insertSpaces: false, tabSize: 1 };
  return { insertSpaces: true, tabSize: m[1]!.length };
}

/** Add (entry) or remove (undefined) our server in a JSON settings file, keeping comments and other entries. */
export function editJson(file: string, key: string, entry: object | undefined, appName: string): ConnectResult {
  const before = exists(file) ? fs.readFileSync(file, "utf8") : null;
  const src = before !== null && before.trim() ? before : "{}\n";
  const errors: jsoncNs.ParseError[] = [];
  const tree = jsonc.parseTree(src, errors, { allowTrailingComma: true });
  if (errors.length || !tree || tree.type !== "object") return { ok: false, message: cantRead(file) };
  const value = jsonc.getNodeValue(tree) as Record<string, unknown>;
  const servers = value[key];
  if (servers !== undefined && (servers === null || typeof servers !== "object" || Array.isArray(servers)))
    return { ok: false, message: cantRead(file) };
  const current = (servers as Record<string, unknown> | undefined)?.[ENTRY];

  if (entry && same(current, entry)) return { ok: true, message: `Context Capsule is already connected to ${appName}.` };
  if (!entry && current === undefined) return { ok: true, message: `Context Capsule wasn't connected to ${appName}.` };

  const eol = src.includes("\r\n") ? "\r\n" : "\n";
  const edits = jsonc.modify(src, [key, ENTRY], entry, { formattingOptions: { ...indentOf(src), eol } });
  let after = jsonc.applyEdits(src, edits);
  if (!after.endsWith(eol)) after += eol;
  const check: jsoncNs.ParseError[] = [];
  const parsed = jsonc.parse(after, check, { allowTrailingComma: true }) as Record<string, Record<string, unknown>>;
  if (check.length || !same(parsed?.[key]?.[ENTRY], entry)) return { ok: false, message: cantRead(file) };
  writeWithBackup(file, before, after);
  return entry
    ? { ok: true, message: `Connected to ${appName}. Restart ${appName} to use Context Capsule.` }
    : { ok: true, message: `Disconnected from ${appName}.` };
}

const TOML_HEADER = /^\s*\[\[?\s*([^\]]*?)\s*\]\]?\s*(#.*)?$/;
const OUR_TABLE = new RegExp(`^mcp_servers\\s*\\.\\s*(?:${ENTRY}|"${ENTRY}"|'${ENTRY}')(\\s*\\..*)?$`);

/** The TOML text without our [mcp_servers.context-capsule] table (and its sub-tables). */
export function removeTomlTable(text: string): string {
  const lines = text.split(/\r?\n/);
  const out: string[] = [];
  let skipping = false;
  // comments after our last key belong to whatever comes next: keep them
  let pending: string[] = [];
  for (const line of lines) {
    const h = TOML_HEADER.exec(line);
    if (h) {
      const ours = OUR_TABLE.test(h[1]!);
      if (ours && !skipping) while (out.length && !out[out.length - 1]!.trim()) out.pop();
      if (!ours && skipping) {
        while (pending.length && !pending[0]!.trim()) pending.shift();
        if (out.length) out.push("");
        out.push(...pending);
      }
      pending = [];
      skipping = ours;
    }
    if (!skipping) out.push(line);
    else if (!h) pending = /^\s*(#.*)?$/.test(line) ? [...pending, line] : [];
  }
  if (skipping) out.push(...pending.filter((l) => l.trim()));
  while (out.length && !out[out.length - 1]!.trim()) out.pop();
  return out.length ? out.join("\n") + "\n" : "";
}

const tomlString = (s: string) => JSON.stringify(s);

function withoutOurs(data: Record<string, unknown>): Record<string, unknown> {
  const copy = JSON.parse(JSON.stringify(data)) as Record<string, unknown>;
  const servers = copy.mcp_servers as Record<string, unknown> | undefined;
  if (servers && typeof servers === "object") {
    delete servers[ENTRY];
    if (!Object.keys(servers).length) delete copy.mcp_servers;
  }
  return copy;
}

/** Add (entry) or remove (undefined) our server in a TOML file ([mcp_servers.<name>]), keeping everything else. */
export function editToml(file: string, entry: SelfCommand | undefined, appName: string): ConnectResult {
  const before = exists(file) ? fs.readFileSync(file, "utf8") : null;
  const src = before ?? "";
  let data: Record<string, unknown>;
  try {
    data = parseToml(src) as Record<string, unknown>;
  } catch {
    return { ok: false, message: cantRead(file) };
  }
  const servers = data.mcp_servers;
  if (servers !== undefined && (typeof servers !== "object" || servers === null || Array.isArray(servers)))
    return { ok: false, message: cantRead(file) };
  const current = (servers as Record<string, unknown> | undefined)?.[ENTRY] as Record<string, unknown> | undefined;
  if (entry && current && same({ command: current.command, args: current.args }, entry))
    return { ok: true, message: `Context Capsule is already connected to ${appName}.` };
  if (!entry && current === undefined) return { ok: true, message: `Context Capsule wasn't connected to ${appName}.` };

  let after = removeTomlTable(src.replace(/\r\n/g, "\n"));
  if (entry) {
    const block = `[mcp_servers.${ENTRY}]\ncommand = ${tomlString(entry.command)}\nargs = [${entry.args.map(tomlString).join(", ")}]\n`;
    after = after.trim() ? `${after.replace(/\n*$/, "\n")}\n${block}` : block;
  }
  if (src.includes("\r\n")) after = after.replace(/\n/g, "\r\n");
  // Only write if everything else reads back exactly the same, and our entry is right.
  let back: Record<string, unknown>;
  try {
    back = parseToml(after) as Record<string, unknown>;
  } catch {
    return { ok: false, message: cantRead(file) };
  }
  const ours = (back.mcp_servers as Record<string, unknown> | undefined)?.[ENTRY];
  if (!same(withoutOurs(back), withoutOurs(data)) || !same(ours, entry)) return { ok: false, message: cantRead(file) };
  writeWithBackup(file, before, after);
  return entry
    ? { ok: true, message: `Connected to ${appName}. Restart ${appName} to use Context Capsule.` }
    : { ok: true, message: `Disconnected from ${appName}.` };
}

function jsonEntry(def: ClientDef, self: SelfCommand): object {
  const f = def.format as Extract<Format, { kind: "json" }>;
  return f.vscode ? { type: "stdio", command: self.command, args: self.args } : { command: self.command, args: self.args };
}

export function connect(id: string, opts: { ctx?: Ctx; self?: SelfCommand } = {}): ConnectResult {
  return change(id, true, opts);
}

export function disconnect(id: string, opts: { ctx?: Ctx } = {}): ConnectResult {
  return change(id, false, opts);
}

function change(id: string, on: boolean, opts: { ctx?: Ctx; self?: SelfCommand }): ConnectResult {
  const c = opts.ctx ?? defaultCtx();
  const def = CLIENTS.find((d) => d.id === id.trim().toLowerCase());
  if (!def) return { ok: false, message: `"${id}" isn't an AI app Context Capsule knows. Known: ${CLIENTS.map((d) => d.id).join(", ")}.` };
  if (def.how === "extension")
    return {
      ok: false,
      how: "extension",
      message: on
        ? "Claude uses the Context Capsule extension: open context-capsule.mcpb to add it to Claude."
        : "Claude uses the Context Capsule extension: remove it in Claude → Settings → Extensions.",
    };
  if (on && !def.installed(c)) return { ok: false, message: `${def.name} isn't on this computer.` };
  const file = def.file!(c);
  const self = opts.self ?? selfCommand();
  // Never write (and call "connected") an entry that can't start: listClients would say not connected, rightly.
  if (on && !opts.self && !pointsAtExisting(self)) return { ok: false, message: `Context Capsule couldn't find its own program, so it can't connect ${def.name}. Please reinstall Context Capsule.` };
  try {
    if (def.format!.kind === "toml") return editToml(file, on ? self : undefined, def.name);
    return editJson(file, def.format!.key, on ? jsonEntry(def, self) : undefined, def.name);
  } catch (e) {
    return { ok: false, message: `Couldn't change ${file}: ${(e as Error).message}` };
  }
}
