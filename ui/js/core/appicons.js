// context-capsule — ui: app icons from the host (bridge `apps.icons`), asked once per app and kept for the page's
// life. Apps drawn in the same moment go in one call; an app the host has no icon for (or a host without the
// method) keeps its glyph.

/** The key the host answers by: the app's id (bundle id, exe, desktop id), else its name. */
export function iconKey(app, bundle) {
  return String(bundle ?? "").trim() || String(app ?? "").trim();
}

/** A cache over `fetch(apps) → Promise<{ key: dataURL | null }>`. */
export function makeIconCache(fetch) {
  const known = new Map(); // key → data URL | null
  const waiting = new Map(); // key → { app, bundle, fns, sent }
  let scheduled = null;

  /**
   * The icon for an app: a data URL, null (none), or undefined (being asked; `fn(url | null)` is called when the
   * answer comes).
   */
  function want(app, bundle, fn) {
    const key = iconKey(app, bundle);
    if (!key) return null;
    if (known.has(key)) return known.get(key);
    let w = waiting.get(key);
    if (!w) waiting.set(key, (w = { app: String(app ?? "").trim(), bundle: String(bundle ?? "").trim(), fns: [], sent: false }));
    if (fn) w.fns.push(fn);
    if (!w.sent && !scheduled) scheduled = Promise.resolve().then(flush);
    return undefined;
  }

  /** Asks the host for every app wanted since the last call. */
  async function flush() {
    scheduled = null;
    const batch = [...waiting].filter(([, w]) => !w.sent);
    if (!batch.length) return;
    for (const [, w] of batch) w.sent = true;
    let answer;
    try {
      answer = (await fetch(batch.map(([, w]) => ({ app: w.app, bundle: w.bundle })))) || {};
    } catch {
      answer = {};
    }
    for (const [key, w] of batch) {
      const url = typeof answer[key] === "string" && answer[key].startsWith("data:image/") ? answer[key] : null;
      known.set(key, url);
      waiting.delete(key);
      for (const fn of w.fns) {
        try {
          fn(url);
        } catch {}
      }
    }
  }

  return { want, flush, get: (app, bundle) => known.get(iconKey(app, bundle)) };
}
