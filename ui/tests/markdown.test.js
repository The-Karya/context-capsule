// context-capsule — ui: tests for the reading view's Markdown (blocks and inline pieces) and its words
import { test, expect } from "bun:test";
import { blocks, inline } from "../js/core/markdown.js";
import * as Caps from "../js/core/capsules.js";

test("blocks: headings, paragraphs, lists, quotes, code", () => {
  const md = "## In short\nRedesigned the page.\nTeams moves to $12.\n\n## Decisions\n- **Annual** — default\n  wrapped\n* two\n\n1. one\n2. two\n\n> said\n```\ncode\n```\n";
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

test("reading view words", () => {
  expect(Caps.continueSavedPrompt("website.capsule.md")).toBe("Use Context Capsule to continue from my capsule website.capsule.md.");
  expect(Caps.showLabel("mac")).toBe("Show in Finder");
  expect(Caps.showLabel("windows")).toBe("Show in folder");
  expect(Caps.showLabel("linux")).toBe("Show in Files");
  expect(Caps.pastePrompt("TEXT").endsWith("\n\nTEXT")).toBe(true);
});
