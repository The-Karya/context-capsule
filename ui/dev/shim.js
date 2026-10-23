// context-capsule — ui: dev shim — window.capsuleHost over HTTP: calls POST /bridge, events come from /events (SSE).
(() => {
  const listeners = new Map(); // event -> Set of fns
  const last = new Map(); // latest `state` / `appearance`, replayed to late subscribers
  const query = location.search;

  const host = {
    // Resolves `value`, or rejects with an Error whose message is the host's plain-words `error`.
    async call(method, args = {}) {
      const res = await fetch("/bridge", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ method, args: args ?? {}, query }),
      });
      const answer = await res.json();
      if (answer.ok) return answer.value;
      throw new Error(answer.error);
    },
    on(event, fn) {
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event).add(fn);
      if (last.has(event)) queueMicrotask(() => listeners.get(event)?.has(fn) && fn(last.get(event)));
      return () => listeners.get(event)?.delete(fn);
    },
    _emit(event, payload) {
      if (event === "state" || event === "appearance") last.set(event, payload);
      for (const fn of [...(listeners.get(event) ?? [])]) {
        try { fn(payload); } catch (e) { console.error(`capsuleHost ${event} listener:`, e); }
      }
    },
  };
  window.capsuleHost = host;

  // The server sends `state` on connect and after host.hello; other events before anyone listens are dropped.
  const source = new EventSource("/events" + query);
  source.onmessage = (m) => {
    try {
      const { event, payload } = JSON.parse(m.data);
      host._emit(event, payload);
    } catch (e) { console.error("capsuleHost bad event", e); }
  };
})();

// Dev only: show script errors on the page (renders have no console).
(function () {
  const show = (msg) => {
    const d = document.createElement("pre");
    d.style.cssText = "position:fixed;left:0;right:0;bottom:0;z-index:99;background:#c00;color:#fff;font:11px monospace;margin:0;padding:6px;white-space:pre-wrap";
    d.textContent = msg;
    (document.body || document.documentElement).append(d);
    fetch("/bridge", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ method: "dev.log", args: { msg } }) }).catch(() => {});
  };
  window.addEventListener("error", (e) => show(`${e.message} @ ${e.filename}:${e.lineno}`));
  window.addEventListener("unhandledrejection", (e) => show("unhandled: " + (e.reason && (e.reason.stack || e.reason.message) || e.reason)));
})();
