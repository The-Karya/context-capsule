// context-capsule — mcp: the MCP server over Streamable HTTP, for local apps that only take a URL. Listens on
// 127.0.0.1 only, needs the token in ~/.context-capsule/mcp-token as "Authorization: Bearer <token>", and answers
// no other web page (no CORS; foreign Origin or Host headers are refused). Nothing is reachable from outside.
import { randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import type { AddressInfo } from "node:net";
import path from "node:path";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { isInitializeRequest } from "@modelcontextprotocol/sdk/types.js";
import { capsuleHome } from "./paths.js";
import { createServer, type ServerOptions } from "./server.js";

export const DEFAULT_PORT = 3740;
export const HOST = "127.0.0.1";
export const MCP_PATH = "/mcp";

export const tokenFile = () => path.join(capsuleHome(), "mcp-token");

/** The token, made on first use (random, readable by this user only). */
export function readOrCreateToken(): string {
  const file = tokenFile();
  try {
    const t = fs.readFileSync(file, "utf8").trim();
    if (t.length >= 32) return t;
  } catch {
    // not made yet
  }
  const token = randomBytes(32).toString("hex");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${token}\n`, { mode: 0o600 });
  try {
    fs.chmodSync(file, 0o600);
  } catch {
    // Windows: the user's profile folder is private already
  }
  return token;
}

function tokenOk(header: string | undefined, token: string): boolean {
  const m = /^Bearer\s+(\S+)\s*$/i.exec(header ?? "");
  if (!m) return false;
  const a = Buffer.from(m[1]!);
  const b = Buffer.from(token);
  return a.length === b.length && timingSafeEqual(a, b);
}

function deny(res: http.ServerResponse, status: number, message: string): void {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify({ jsonrpc: "2.0", error: { code: -32000, message }, id: null }));
}

async function readBody(req: http.IncomingMessage, limit = 8 * 1024 * 1024): Promise<unknown> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const c of req) {
    size += (c as Buffer).length;
    if (size > limit) throw new Error("too large");
    chunks.push(c as Buffer);
  }
  const text = Buffer.concat(chunks).toString("utf8");
  return text ? JSON.parse(text) : undefined;
}

export interface HttpServer {
  url: string;
  port: number;
  close: () => Promise<void>;
}

/** Listen on the port; if it's taken and `fallback`, on any free one. */
function listen(server: http.Server, port: number, fallback: boolean): Promise<number> {
  return new Promise((resolve, reject) => {
    const onError = (e: NodeJS.ErrnoException) => {
      if (e.code === "EADDRINUSE" && fallback && port !== 0) {
        server.off("error", onError);
        listen(server, 0, false).then(resolve, reject);
      } else reject(e);
    };
    server.once("error", onError);
    server.listen(port, HOST, () => {
      server.off("error", onError);
      resolve((server.address() as AddressInfo).port);
    });
  });
}

/** Start the HTTP server. `port`: the port to use (0 = any free one); without it 3740, or any free one if taken. */
export async function startHttp(opts: { port?: number; token?: string; server?: ServerOptions } = {}): Promise<HttpServer> {
  const token = opts.token ?? readOrCreateToken();
  const sessions = new Map<string, StreamableHTTPServerTransport>();
  let port = 0;

  const httpServer = http.createServer(async (req, res) => {
    try {
      const url = new URL(req.url ?? "/", `http://${HOST}`);
      // Only this computer's own clients: a web page elsewhere can't reach in (no CORS, Host and Origin checked).
      const host = (req.headers.host ?? "").toLowerCase();
      if (host !== `${HOST}:${port}` && host !== `localhost:${port}`) return deny(res, 403, "Forbidden host.");
      const origin = req.headers.origin;
      if (origin && origin !== `http://${HOST}:${port}` && origin !== `http://localhost:${port}`)
        return deny(res, 403, "Forbidden origin.");
      if (url.pathname !== MCP_PATH) return deny(res, 404, `Not found. The MCP endpoint is ${MCP_PATH}.`);
      if (!tokenOk(req.headers.authorization, token)) {
        res.setHeader("www-authenticate", 'Bearer realm="context-capsule"');
        return deny(res, 401, "Unauthorized: send Authorization: Bearer <token from ~/.context-capsule/mcp-token>.");
      }

      const sid = req.headers["mcp-session-id"];
      const id = Array.isArray(sid) ? sid[0] : sid;
      if (req.method === "POST") {
        let body: unknown;
        try {
          body = await readBody(req);
        } catch {
          return deny(res, 400, "Parse error: the body must be JSON.");
        }
        let transport = id ? sessions.get(id) : undefined;
        if (!transport) {
          if (id) return deny(res, 404, "Session not found. Start again with initialize.");
          if (!isInitializeRequest(body)) return deny(res, 400, "Bad request: no session id. Start with initialize.");
          const t = new StreamableHTTPServerTransport({
            sessionIdGenerator: () => randomUUID(),
            onsessioninitialized: (newId) => {
              sessions.set(newId, t);
            },
          });
          t.onclose = () => {
            if (t.sessionId) sessions.delete(t.sessionId);
          };
          await createServer(opts.server).connect(t);
          transport = t;
        }
        await transport.handleRequest(req, res, body);
        return;
      }
      if (req.method === "GET" || req.method === "DELETE") {
        const transport = id ? sessions.get(id) : undefined;
        if (!transport) return deny(res, id ? 404 : 400, id ? "Session not found." : "Bad request: no session id.");
        await transport.handleRequest(req, res);
        return;
      }
      res.setHeader("allow", "GET, POST, DELETE");
      return deny(res, 405, "Method not allowed.");
    } catch (e) {
      if (!res.headersSent) deny(res, 500, `Internal error: ${(e as Error).message}`);
    }
  });

  port = await listen(httpServer, opts.port ?? DEFAULT_PORT, opts.port === undefined);
  return {
    url: `http://${HOST}:${port}${MCP_PATH}`,
    port,
    close: async () => {
      for (const t of sessions.values()) await t.close().catch(() => {});
      sessions.clear();
      httpServer.closeAllConnections?.();
      await new Promise<void>((r) => httpServer.close(() => r()));
    },
  };
}
