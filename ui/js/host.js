// context-capsule — ui: the bridge to the native host (see ui/BRIDGE.md): call(method, args) and events

const listeners = new Map();

function bridge() {
  const h = window.capsuleHost;
  if (!h) throw new Error("No host: open this page in Context Capsule, or with ui/dev/server.js");
  return h;
}

/** Ask the host; resolves its JSON answer, rejects with its plain words. */
export function call(method, args = {}) {
  try {
    return Promise.resolve(bridge().call(method, args));
  } catch (e) {
    return Promise.reject(e);
  }
}

/** Listen to a host event: "state", "appearance", "data-changed", "navigate", "open-capsule". */
export function on(event, fn) {
  if (!listeners.has(event)) {
    listeners.set(event, new Set());
    try {
      bridge().on(event, (payload) => {
        for (const f of listeners.get(event)) f(payload);
      });
    } catch {}
  }
  listeners.get(event).add(fn);
}

/** The error's words, without "Error: ". */
export function why(e) {
  return e && typeof e === "object" && "message" in e ? e.message : String(e ?? "");
}

/** Raw files from the data folder: { path: text | null }. */
export async function read(files) {
  if (!files.length) return {};
  return call("data.read", { files });
}

export async function list(dir) {
  return (await call("data.list", { dir })) || [];
}
