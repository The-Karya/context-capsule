// context-capsule — ui: Automatic capsules (SPEC "Automatic capsules"): reading auto.status from the host, and what
// the make control shows on Capsules and Home — the button, a quiet status line, or "Making today's capsules…".

export const words = {
  make: "Make today's capsules",
  making: "Making today's capsules…",
  makeNow: "Make now",
  tryAgain: "Try again",
  automatically: "Automatically",
  whenIClick: "When I click",
  makeCapsules: "Make capsules",
  needs: "Needs Claude Code, Codex or Gemini CLI on this computer.",
  learnMore: "Learn more",
  learnMoreUrl: "https://claude.com/claude-code",
  nothingToday: "Nothing recorded today yet.",
};

/** The host's answer, made safe: { mode, at, agent, running, autoDay, last }. Unknown or missing → click, no tool. */
export function parseStatus(v) {
  const o = v && typeof v === "object" ? v : {};
  const at = typeof o.at === "string" && /^\d{1,2}:\d{2}$/.test(o.at) ? o.at.padStart(5, "0") : "18:00";
  const agent = o.agent && typeof o.agent === "object" && typeof o.agent.name === "string" ? { id: String(o.agent.id || ""), name: o.agent.name } : null;
  const l = o.last && typeof o.last === "object" ? o.last : null;
  const last = l && typeof l.day === "string"
    ? {
        day: l.day,
        by: l.by === "auto" ? "auto" : "click",
        status: ["running", "done", "failed"].includes(l.status) ? l.status : "failed",
        startedAt: Date.parse(l.started_at) || null,
        finishedAt: Date.parse(l.finished_at) || null,
        capsules: Number.isFinite(l.capsules) ? l.capsules : 0,
        reason: typeof l.reason === "string" ? l.reason : "",
      }
    : null;
  return {
    mode: o.mode === "auto" ? "auto" : "click",
    at,
    agent,
    running: o.running === true,
    autoDay: typeof o.autoDay === "string" ? o.autoDay : null,
    last,
  };
}

/** "2026-10-23", the local day of a time. */
export function dayOf(ms) {
  const d = new Date(ms);
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** "18:00" today, as a time. */
export function atToday(at, now) {
  const [h, m] = at.split(":").map(Number);
  const d = new Date(now);
  d.setHours(h, m, 0, 0);
  return d.getTime();
}

/**
 * What the make control shows. `time(ms)` writes a clock time ("6:00 PM").
 * → { kind: "button" } | { kind: "running", text } | { kind: "line", text, detail?, action?: "make" | "retry", actionText? }
 * The blue button: When I click, no tool, or Automatically past its time without a run today (want it now).
 */
export function control(st, now, time) {
  if (!st) return { kind: "button" };
  if (st.running) return { kind: "running", text: words.making };
  if (st.mode !== "auto" || !st.agent) return { kind: "button" };
  const today = dayOf(now);
  const at = atToday(st.at, now);
  const last = st.last && st.last.day === today ? st.last : null;
  // a run today that counts: the automatic one, or one started at or after the time
  const counts = last && (st.autoDay === today || (last.startedAt != null && last.startedAt >= at));
  if (counts && last.status === "done") {
    const when = time(last.finishedAt ?? last.startedAt ?? now);
    return { kind: "line", text: last.by === "auto" ? `Made automatically at ${when}` : `Made at ${when}` };
  }
  if (counts && last.status === "failed") {
    return { kind: "line", text: "Couldn't make them", detail: last.reason || "", action: "retry", actionText: words.tryAgain };
  }
  if (now < at) return { kind: "line", text: `Next at ${time(at)}`, action: "make", actionText: words.makeNow };
  return { kind: "button" };
}

/** Settings → Every day, under the card. */
export function settingsNote(st) {
  if (!st || st.mode !== "auto") return null;
  if (!st.agent) return null; // the "Needs…" line with its link is shown instead
  return `${st.agent.name} makes them in the background, using only Context Capsule.`;
}

/** Home → Make today's capsules, the line above the button when a tool makes them. */
export function homeLine(st) {
  return st?.agent ? `${st.agent.name} makes them in the background. Nothing to paste.` : null;
}
