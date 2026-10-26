// context-capsule — mcp: a capsule as a web page anyone can open. One self-contained HTML file: no scripts, no
// network (a Content-Security-Policy forbids loading anything), inline CSS in the plain macOS look, light and dark,
// prints cleanly. Capsule text is untrusted: every piece of it is escaped, links are http(s) only.
import { localDay } from "./activity.js";
import { validateCapsuleText, type FrontMatter } from "./capsules.js";
import { blocks, inline, type Inline } from "./markdown.js";

/** Text safe inside HTML elements and quoted attributes. */
export function escapeHtml(s: string): string {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/** "2026-10-19" → "19 October 2026". Anything else stays as it is. */
export function longDate(day: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  const month = m ? MONTHS[Number(m[2]) - 1] : undefined;
  return m && month ? `${Number(m[3])} ${month} ${m[1]}` : day;
}

function inlineHtml(text: string): string {
  return inline(text)
    .map((p: Inline) => {
      if (p.t === "b") return `<strong>${escapeHtml(p.text)}</strong>`;
      if (p.t === "i") return `<em>${escapeHtml(p.text)}</em>`;
      if (p.t === "code") return `<code>${escapeHtml(p.text)}</code>`;
      if (p.t === "link") {
        const bare = p.text === p.url ? ' class="bare"' : "";
        return `<a href="${escapeHtml(p.url)}" rel="noopener noreferrer"${bare}>${escapeHtml(p.text)}</a>`;
      }
      return escapeHtml(p.text);
    })
    .join("");
}

/** The body as HTML. Headings start at h2 (the page title is the only h1). */
export function bodyHtml(md: string): string {
  return blocks(md)
    .map((b) => {
      switch (b.type) {
        case "h": {
          const n = Math.min(6, Math.max(2, b.level));
          return `<h${n}>${inlineHtml(b.text)}</h${n}>`;
        }
        case "p":
          return `<p>${inlineHtml(b.text)}</p>`;
        case "ul":
        case "ol":
          return `<${b.type}>${b.items.map((i) => `<li>${inlineHtml(i)}</li>`).join("")}</${b.type}>`;
        case "quote":
          return `<blockquote><p>${inlineHtml(b.text)}</p></blockquote>`;
        case "code":
          return `<pre><code>${escapeHtml(b.text)}</code></pre>`;
      }
    })
    .join("\n");
}

const str = (v: unknown) => (typeof v === "string" ? v.trim() : "");

/** "By Saachi · 19 October 2026 · Project: website" from the front matter. */
function metaLine(f: FrontMatter): string {
  const parts: string[] = [];
  if (str(f.author)) parts.push(`By ${str(f.author)}`);
  const created = str(f.created);
  const day = str(f.date) || (created && !Number.isNaN(Date.parse(created)) ? localDay(created) : "");
  if (day) parts.push(f.kind === "project" ? `Updated ${longDate(day)}` : longDate(day));
  if (str(f.project)) parts.push(`${f.kind === "project" ? "Whole project" : "Project"}: ${str(f.project)}`);
  return parts.join(" · ");
}

const CSS = `:root{color-scheme:light dark;--text:#1d1d1f;--muted:#6e6e73;--line:#d2d2d7;--bg:#fff;--code:#f5f5f7;--link:#0066cc;--note:#f5f5f7}
@media (prefers-color-scheme:dark){:root{--text:#f5f5f7;--muted:#a1a1a6;--line:#424245;--bg:#1e1e1e;--code:#2c2c2e;--link:#4ea1ff;--note:#2c2c2e}}
*{box-sizing:border-box}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--text);font:16px/1.55 -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,Roboto,"Helvetica Neue",Arial,sans-serif;overflow-wrap:anywhere}
main{max-width:44rem;margin:0 auto;padding:3rem 1.25rem 4rem}
header{margin-bottom:1.5rem}
h1{font-size:1.75rem;line-height:1.25;font-weight:600;margin:0 0 .35rem}
.meta{color:var(--muted);font-size:.875rem;margin:0}
.about{color:var(--muted);font-size:.8125rem;margin:.75rem 0 0}
.draft{background:var(--note);border-radius:6px;padding:.5rem .75rem;font-size:.875rem;margin:.75rem 0 0}
h2{font-size:1.125rem;font-weight:600;margin:1.75rem 0 .5rem;padding-top:1.25rem;border-top:1px solid var(--line)}
h3,h4,h5,h6{font-size:1rem;font-weight:600;margin:1.25rem 0 .35rem}
p,ul,ol,blockquote,pre{margin:0 0 .75rem}
ul,ol{padding-left:1.4rem}
li{margin:.2rem 0}
blockquote{margin-left:0;padding-left:.9rem;border-left:3px solid var(--line);color:var(--muted)}
code,pre{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,"Liberation Mono",monospace;font-size:.875em;background:var(--code);border-radius:4px}
code{padding:.1em .3em}
pre{padding:.75rem;white-space:pre-wrap}
pre code{background:none;padding:0;font-size:1em}
a{color:var(--link)}
em{color:var(--muted)}
@media print{:root{--text:#000;--muted:#444;--line:#bbb;--bg:#fff;--code:#f2f2f2;--link:#000;--note:#f2f2f2}
body{font-size:11pt}
main{max-width:none;padding:0}
h2{break-after:avoid}
li,blockquote,pre{break-inside:avoid}
a{text-decoration:none}
a[href]:not(.bare)::after{content:" (" attr(href) ")";color:var(--muted);font-size:.85em}}`;

/** Nothing may load or run: styles only from this page. */
const CSP = "default-src 'none'; style-src 'unsafe-inline'; img-src 'none'; base-uri 'none'; form-action 'none'";

/** A capsule file's text → a whole HTML page. Throws if the text isn't a capsule. */
export function capsuleHtml(text: string): string {
  const v = validateCapsuleText(text);
  if (!v.ok) throw new Error(`This isn't a Context Capsule, so there is nothing to show. ${v.error}`);
  const f = v.front;
  const title = str(f.title);
  const meta = metaLine(f);
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${CSP}">
<meta name="referrer" content="no-referrer">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>${escapeHtml(title)}</title>
<style>${CSS}</style>
</head>
<body>
<main>
<header>
<h1>${escapeHtml(title)}</h1>
${meta ? `<p class="meta">${escapeHtml(meta)}</p>\n` : ""}<p class="about">A Context Capsule: notes about this work, written so an AI can carry it on. You can read it too.</p>
${f.draft === true ? `<p class="draft">Draft: some checks hadn't passed when this was saved, so parts may be incomplete.</p>\n` : ""}</header>
<article>
${bodyHtml(v.body.trim())}
</article>
</main>
</body>
</html>
`;
}
