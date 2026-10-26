// context-capsule — mcp: tests for the capsule Markdown reader (the same cases as ui/tests/markdown.test.js)
import { expect, test } from "bun:test";
import { blocks, inline } from "../src/markdown.ts";

test("blocks: headings, paragraphs, lists, quotes, code", () => {
  const md =
    "## In short\nRedesigned the page.\nTeams moves to $12.\n\n## Decisions\n- **Annual** — default\n  wrapped\n* two\n\n1. one\n2. two\n\n> said\n```\ncode\n```\n";
  expect(blocks(md)).toEqual([
    { type: "h", level: 2, text: "In short" },
    { type: "p", text: "Redesigned the page. Teams moves to $12." },
    { type: "h", level: 2, text: "Decisions" },
    { type: "ul", items: ["**Annual** — default wrapped", "two"] },
    { type: "ol", items: ["one", "two"] },
    { type: "quote", text: "said" },
    { type: "code", text: "code" },
  ]);
});

test("blocks: Windows line ends and ~~~ fences", () => {
  expect(blocks("## A\r\n~~~\r\nx ``` y\r\n~~~\r\n")).toEqual([
    { type: "h", level: 2, text: "A" },
    { type: "code", text: "x ``` y" },
  ]);
});

test("inline: bold, italic citations, code, links (http only), trailing punctuation", () => {
  expect(inline("**Annual is the default** — monthly. *(you said, 10:14)*")).toEqual([
    { t: "b", text: "Annual is the default" },
    { t: "text", text: " — monthly. " },
    { t: "i", text: "(you said, 10:14)" },
  ]);
  expect(inline("See https://acme.com/a, or [docs](https://x.io/d) and `npm i`.")).toEqual([
    { t: "text", text: "See " },
    { t: "link", text: "https://acme.com/a", url: "https://acme.com/a" },
    { t: "text", text: "," },
    { t: "text", text: " or " },
    { t: "link", text: "docs", url: "https://x.io/d" },
    { t: "text", text: " and " },
    { t: "code", text: "npm i" },
    { t: "text", text: "." },
  ]);
  expect(inline("[x](javascript:alert(1)) snake_case_name 2*3*4")).toEqual([{ t: "text", text: "[x](javascript:alert(1)) snake_case_name 2*3*4" }]);
});
