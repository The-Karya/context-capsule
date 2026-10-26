// context-capsule — mcp: the context-capsule-mcp commands. `serve` (or nothing) is the MCP server; every other
// command prints JSON on stdout for the apps: clients, connect, disconnect, day, save, agents, make, view.
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { chatDay, saveReply } from "./anyai.js";
import { agentsInfo, make } from "./auto.js";
import { connect, disconnect, listClients } from "./clients.js";
import { checkDay } from "./day.js";
import { DEFAULT_PORT, startHttp, tokenFile } from "./http.js";
import { MAKE_TOOLS, VERSION, createServer } from "./server.js";
import { viewCapsule } from "./viewer.js";

export const USAGE = `context-capsule-mcp ${VERSION} — Context Capsule for any AI

  serve                      the MCP server on stdio (the default)
  serve --make-only          only the tools that write a day's capsules (used by "make")
  serve --http [--port N]    the MCP server over HTTP on 127.0.0.1 (port ${DEFAULT_PORT} if free); needs the token in
                             ~/.context-capsule/mcp-token as "Authorization: Bearer <token>"
  clients                    the AI apps on this computer, as JSON
  connect <id>               add Context Capsule to that app's MCP settings (keeps a .bak)
  disconnect <id>            remove it again
  day [--date YYYY-MM-DD]    one paste-ready message for any AI chat: {"text","chars"}
  save [--date YYYY-MM-DD]   read the AI's answer on stdin and save its capsules: {"saved","refused"}
  agents                     the AI command-line tools found here (Claude Code, Codex, Gemini CLI): {"agents","use"}
  make [--date D] [--by auto|click]
                             make the day's capsules with one of them, headless, with only Context Capsule's tools;
                             writes auto.json: {"ok","status","capsules","reason"}
  view <file or name> [--to path]
                             a capsule as a web page anyone can open (next to the file, or on the Desktop): {"ok","path"}
  --version                  the version`;

/** "--date 2026-10-19" or "--date=2026-10-19". */
function flag(args: string[], name: string): string | undefined {
  for (let i = 0; i < args.length; i++) {
    const a = args[i]!;
    if (a === `--${name}`) return args[i + 1];
    if (a.startsWith(`--${name}=`)) return a.slice(name.length + 3);
  }
  return undefined;
}

async function readStdin(): Promise<string> {
  if (process.stdin.isTTY) return "";
  const chunks: Buffer[] = [];
  for await (const c of process.stdin) chunks.push(typeof c === "string" ? Buffer.from(c) : (c as Buffer));
  return Buffer.concat(chunks).toString("utf8");
}

/** Write all of it before the process ends (stdout can be a pipe that fills up). */
function print(value: unknown): Promise<void> {
  return new Promise((resolve) => process.stdout.write(`${JSON.stringify(value)}\n`, () => resolve()));
}

async function serve(args: string[]): Promise<void> {
  if (args.includes("--http")) {
    const raw = flag(args, "port");
    const port = raw === undefined ? undefined : Number(raw);
    if (port !== undefined && !(Number.isInteger(port) && port >= 0 && port < 65536)) throw new Error(`"${raw}" is not a port number.`);
    const s = await startHttp({ port });
    console.error(`context-capsule MCP server on ${s.url} (only this computer; token in ${tokenFile()})`);
    const stop = () => void s.close().then(() => process.exit(0));
    process.on("SIGINT", stop);
    process.on("SIGTERM", stop);
    return;
  }
  const server = createServer(args.includes("--make-only") ? { only: MAKE_TOOLS } : {});
  await server.connect(new StdioServerTransport());
  // stdout is the protocol channel; logs go to stderr
  console.error("context-capsule MCP server running on stdio");
}

/** Run one command. Returns when it's done (serve keeps running). */
export async function main(argv: string[]): Promise<void> {
  const [cmd = "serve", ...args] = argv;
  switch (cmd) {
    case "serve":
      return serve(args);
    case "clients":
      return print(listClients());
    case "connect":
    case "disconnect": {
      const id = args[0];
      if (!id) {
        process.exitCode = 2;
        return print({ ok: false, message: `Say which app: ${cmd} <id> (see "clients").` });
      }
      const r = cmd === "connect" ? connect(id) : disconnect(id);
      if (!r.ok) process.exitCode = 1;
      return print(r);
    }
    case "day": {
      try {
        return await print(chatDay(checkDay(flag(args, "date"))));
      } catch (e) {
        process.exitCode = 1;
        return print({ error: (e as Error).message });
      }
    }
    case "save": {
      try {
        const date = flag(args, "date");
        return await print(saveReply(await readStdin(), { date: date ? checkDay(date) : undefined }));
      } catch (e) {
        process.exitCode = 1;
        return print({ error: (e as Error).message });
      }
    }
    case "agents":
      return print(agentsInfo());
    case "make": {
      try {
        const by = flag(args, "by") === "auto" ? "auto" : "click";
        const r = await make({ day: checkDay(flag(args, "date")), by });
        if (!r.ok) process.exitCode = 1;
        return await print(r);
      } catch (e) {
        process.exitCode = 1;
        return print({ ok: false, status: "failed", error: (e as Error).message });
      }
    }
    case "view": {
      const from = args.find((a, i) => !a.startsWith("--") && args[i - 1] !== "--to");
      if (!from) {
        process.exitCode = 2;
        return print({ ok: false, message: "Say which capsule: view <file or name> [--to path]." });
      }
      try {
        return await print({ ok: true, path: viewCapsule(from, flag(args, "to")) });
      } catch (e) {
        process.exitCode = 1;
        return print({ ok: false, message: (e as Error).message });
      }
    }
    case "--version":
    case "-v":
    case "version":
      process.stdout.write(`${VERSION}\n`);
      return;
    case "--help":
    case "-h":
    case "help":
      process.stdout.write(`${USAGE}\n`);
      return;
    default:
      process.exitCode = 2;
      process.stderr.write(`Unknown command "${cmd}".\n\n${USAGE}\n`);
  }
}
