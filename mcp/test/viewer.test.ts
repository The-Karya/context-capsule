// context-capsule — mcp: tests for the capsule web page (structure, escaping, no scripts or network, dates)
import { describe, expect, test } from "bun:test";
import { stringifyFrontMatter } from "../src/capsules.ts";
import { capsuleHtml, escapeHtml, longDate } from "../src/viewer.ts";
import { VALID_BODY } from "./helpers.ts";

const FRONT = { capsule: 1, title: "Pricing page redesign", author: "Saachi", created: "2026-10-19T11:30:00Z", project: "website", kind: "daily", date: "2026-10-19" };
const page = (body: string, front: Record<string, unknown> = FRONT) =>
  capsuleHtml(`${stringifyFrontMatter(front as never)}\n${body}`);

describe("capsule page", () => {
  test("a whole page: title, who and when, every section, lists and citations", () => {
    const html = page(VALID_BODY);
    expect(html.startsWith("<!doctype html>")).toBe(true);
    expect(html).toContain("<title>Pricing page redesign</title>");
    expect(html).toContain("<h1>Pricing page redesign</h1>");
    expect(html).toContain('<p class="meta">By Saachi · 19 October 2026 · Project: website</p>');
    for (const s of ["In short", "For Claude", "Where things stand", "Decisions", "Next steps"]) expect(html).toContain(`<h2>${s}</h2>`);
    expect(html).toContain("<li><strong>Teams at $12/seat</strong> — matches competitors. <em>(seen in Linear, 10:08)</em></li>");
    expect(html).toContain("<ol><li>Ship the page</li></ol>");
  });

  test("self-contained: no scripts, nothing loaded, light and dark, print styles", () => {
    const html = page(VALID_BODY);
    expect(html).not.toMatch(/<script|<link|<img|<iframe|@import|url\(/i);
    expect(html).toContain(`content="default-src 'none'; style-src 'unsafe-inline'`);
    expect(html).toContain("prefers-color-scheme:dark");
    expect(html).toContain("@media print");
    expect(html).toContain("-apple-system");
    expect(html).not.toMatch(/gradient/i);
  });

  test("capsule text can't add HTML, scripts, styles or odd links", () => {
    const evil =
      `## In short\n<script>alert(1)</script> <img src=x onerror=alert(1)> </style><style>body{display:none}</style>\n` +
      `## Decisions\n- **<b onclick="x">bold</b>** — [click](javascript:alert(1)) [ok](https://a.example/?q="><script>)\n` +
      "```\n</pre><script>alert(2)</script>\n```\n> <iframe src=https://evil.example>\n";
    const html = page(evil, { ...FRONT, title: `</title><script>alert(3)</script>`, author: `"><svg onload=alert(4)>` });
    const body = html.slice(html.indexOf("<body>"));
    expect(body).not.toMatch(/<(script|img|style|iframe|svg|b)\b/i);
    expect(html.match(/<script/gi)).toBeNull();
    expect(html.match(/<style>/g)!.length).toBe(1);
    expect(body).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(body).not.toContain('href="javascript');
    expect(body).toContain('href="https://a.example/?q=&quot;&gt;&lt;script&gt;" rel="noopener noreferrer"');
    expect(html).toContain("<title>&lt;/title&gt;&lt;script&gt;alert(3)&lt;/script&gt;</title>");
  });

  test("a project capsule, a draft, no date: plain words", () => {
    const html = page(VALID_BODY, { capsule: 1, title: "Website", created: "2026-10-19T11:30:00Z", project: "website", kind: "project", draft: true });
    expect(html).toContain("Updated 19 October 2026 · Whole project: website");
    expect(html).toContain('class="draft"');
    expect(page(VALID_BODY, { capsule: 1, title: "Bare" })).not.toContain('class="meta"');
  });

  test("refuses what isn't a capsule", () => {
    expect(() => capsuleHtml("# just notes\n")).toThrow(/isn't a Context Capsule/);
  });
});

test("helpers", () => {
  expect(escapeHtml(`<a href="x">'&'</a>`)).toBe("&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
  expect(longDate("2026-01-05")).toBe("5 January 2026");
  expect(longDate("soon")).toBe("soon");
});
