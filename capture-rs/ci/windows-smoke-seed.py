# context-capsule — ci: seeds a realistic data folder for the Windows smoke test (windows-smoke.yml):
# three recordings today (one with a voice note, a typed note and a gap), one yesterday, projects and capsules.
# Usage: python windows-smoke-seed.py <data folder>. Prints the day it used (today, or yesterday before 07:00).
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

root = Path(sys.argv[1])
now = datetime.now(timezone.utc).replace(microsecond=0)
day = now if now.hour >= 7 else now - timedelta(days=1)
end_of_work = min(now - timedelta(minutes=5), day.replace(hour=23, minute=0, second=0))
z = lambda t: t.strftime("%Y-%m-%dT%H:%M:%SZ")

PROJECTS = [
    {"id": "website", "name": "Website", "match": ["pricing", "figma.com"]},
    {"id": "context-capsule", "name": "Context Capsule", "match": ["karya-capsule"]},
    {"id": "client-report", "name": "Client report", "match": ["acme q3"]},
]

# (minutes from the session's start, minutes long, app, bundle, title, where, project, text)
CHROME = ("Google Chrome", "chrome.exe")
CODE = ("Visual Studio Code", "Code.exe")
FIGMA = ("Figma", "Figma.exe")
WORD = ("Microsoft Word", "WINWORD.EXE")
OUTLOOK = ("Outlook", "olk.exe")


def read(app, title, where, project, text):
    return (app, title, where, project, text)


SESSIONS = [
    # (hours before the end of work, stretches)
    (6.0, [
        (0, 25, *read(CHROME, "Pricing — Linear", "https://linear.app/karya/issue/WEB-142", "website",
                      "WEB-142 Pricing page redesign\nMove Teams plan to $12 per seat, annual only\nAcceptance: the toggle defaults to annual")),
        (25, 40, *read(FIGMA, "Pricing v2 – Figma", "https://www.figma.com/design/pricing-v2", "website",
                       "Pricing v2\nFree · Teams $12 · Business $24\nCompare plans table, FAQ under it\nAnnual / Monthly toggle")),
        (66, 12, *read(OUTLOOK, "Inbox — Outlook", "", None,
                       "Re: Q3 numbers — can you send the draft by Friday?\nLunch on Thursday?")),
    ]),
    (3.6, [
        (0, 50, *read(CODE, "layout.rs — karya-capsule — Visual Studio Code", r"C:\src\karya-capsule\capture-rs\windows\src\layout.rs", "context-capsule",
                      "pub fn show_arg(args: &[String]) -> Option<ShowArg> {\n    let i = args.iter().position(|a| a == \"--show\")?;\nrunning 83 tests ... ok")),
        (52, 30, *read(CHROME, "Actions · The-Karya/context-capsule", "https://github.com/The-Karya/context-capsule/actions", "context-capsule",
                       "windows-smoke: in progress\nci: success · 21 min\nVersion 0.2.0")),
    ]),
    (1.5, [
        (0, 35, *read(WORD, "Acme Q3 report.docx - Word", r"C:\Users\runner\Documents\Acme Q3 report.docx", "client-report",
                      "Acme Q3 report\nRevenue grew 18% over Q2, mostly from the Teams plan\nNext: churn by cohort, due Friday")),
        (37, 20, *read(CHROME, "Acme Q3 dashboard — Looker", "https://acme.looker.com/dashboards/q3", "client-report",
                       "Q3 revenue $1.24M (+18%)\nChurn 3.1% (down from 3.8%)\nActive seats 4,210")),
    ]),
]


def write_session(start, stretches, notes=(), gaps=()):
    sid = start.strftime("%Y%m%d-%H%M%S")
    folder = root / "sessions" / sid
    folder.mkdir(parents=True, exist_ok=True)
    lines = []
    last = start
    for off, mins, (app, bundle), title, where, project, text in stretches:
        t = start + timedelta(minutes=off)
        stop = t + timedelta(minutes=mins)
        first = True
        while t < stop:
            nxt = min(t + timedelta(seconds=90), stop)
            ev = {"t": z(t), "end": z(nxt), "src": "ax", "app": app, "bundle": bundle, "title": title}
            if where:
                ev["where"] = where
            if project:
                ev["project"] = project
                ev["project_by"] = "auto"
            ev["text"] = text if first else ""
            lines.append(ev)
            first = False
            t = nxt
        last = max(last, stop)
    for ev in notes:
        lines.append(ev)
    lines.sort(key=lambda e: e["t"])
    (folder / "events.jsonl").write_text("".join(json.dumps(e, ensure_ascii=False) + "\n" for e in lines), encoding="utf-8")
    if gaps:
        (folder / "gaps.jsonl").write_text("".join(json.dumps(g) + "\n" for g in gaps), encoding="utf-8")
    session = {"id": sid, "started_at": z(start), "ended_at": z(last), "host": "windows", "version": 1}
    (folder / "session.json").write_text(json.dumps(session), encoding="utf-8")
    return sid


ids = []
for i, (hours, stretches) in enumerate(SESSIONS):
    start = end_of_work - timedelta(hours=hours)
    notes, gaps = [], []
    if i == 0:
        v = start + timedelta(minutes=30)
        notes.append({"t": z(v), "end": z(v + timedelta(seconds=17)), "kind": "voice", "src": "voice", "app": "Figma",
                      "bundle": "Figma.exe", "title": "Pricing v2 – Figma", "project": "website", "project_by": "auto",
                      "text": "We keep annual as the default. Monthly is there but costs twenty percent more, so the toggle starts on annual."})
        g = start + timedelta(minutes=78)
        gaps.append({"t": z(g), "end": z(g + timedelta(minutes=6)), "why": "private_app"})
    if i == 1:
        n = start + timedelta(minutes=51)
        notes.append({"t": z(n), "end": z(n), "kind": "note", "src": "typed", "app": "", "bundle": "", "title": "",
                      "project": "context-capsule", "project_by": "auto", "text": "Check the Timeline on a small window before the demo."})
    ids.append(write_session(start, stretches, notes, gaps))

# Yesterday (a dot on the calendar).
y = (day - timedelta(days=1)).replace(hour=14, minute=5, second=0)
write_session(y, SESSIONS[2][1])

(root / "projects.json").write_text(json.dumps({"projects": PROJECTS}, indent=2), encoding="utf-8")
(root / "config.json").write_text(json.dumps({"author": "Saachi"}), encoding="utf-8")

caps = root / "capsules"
caps.mkdir(exist_ok=True)
ymd = day.strftime("%Y-%m-%d")
yest = (day - timedelta(days=1)).strftime("%Y-%m-%d")


def capsule(name, title, project, kind, date, sessions, in_short, extra=""):
    front = [
        "---", "capsule: 1", f"title: {title}", "author: Saachi", f"created: {date}T17:30:00Z",
        f"sessions: [{', '.join(sessions)}]", "apps: [Figma, Google Chrome, Visual Studio Code]",
        f"project: {project}", f"kind: {kind}",
    ]
    front += [f"date: {date}"] if kind == "daily" else [f"days: [{yest}, {ymd}]"]
    body = f"""---

## In short
{in_short}

## For Claude
This is Saachi's work. Treat the decisions as settled and carry on from Next steps.

## Where things stand
The pricing page design is done in Figma; the build hasn't started.

## What was done
- Redesigned the pricing page (Free, Teams $12, Business $24).
{extra}
## Decisions
- **Annual is the default** — monthly costs 20% more. *(you said, 10:14)*

## Open questions
- Do we show a currency switcher?

## Next steps
- Build the page from the Figma file.

## People, links and files
- https://www.figma.com/design/pricing-v2
"""
    (caps / f"{name}.capsule.md").write_text("\n".join(front) + "\n" + body, encoding="utf-8")


capsule(f"website-{ymd}", f"Website — {ymd}", "website", "daily", ymd, ids[:1],
        "Redesigned the pricing page: Teams moves to $12 a seat, annual by default.")
capsule("website", "Website", "website", "project", ymd, ids[:1],
        "The new pricing page is designed and ready to build; annual billing is the default.")
capsule(f"client-report-{yest}", f"Client report — {yest}", "client-report", "daily", yest, [ids[2]],
        "Drafted the Acme Q3 report: revenue up 18%, churn down to 3.1%. Cohort churn is still to do.")

print(ymd)
print(ids[0])
