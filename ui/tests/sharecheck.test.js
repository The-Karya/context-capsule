// context-capsule — ui: tests for Check before sharing (every case in spec/share-check.json, plus edges)
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { find, remove, body, nonEmpty, KINDS, isPrivateLink } from "../js/core/sharecheck.js";

const cases = JSON.parse(readFileSync(new URL("../../spec/share-check.json", import.meta.url), "utf8"));
const byKind = (groups) => Object.fromEntries(groups.map((g) => [g.kind, g.items]));

for (const c of cases.find) {
  test(`find: ${c.name}`, () => {
    expect(byKind(find(c.text))).toEqual(c.expect);
  });
}

cases.remove.forEach((c, i) => {
  test(`remove case ${i + 1}`, () => {
    expect(remove(c.text, c.remove)).toBe(c.expect);
  });
});

test("groups in order with their titles", () => {
  expect(KINDS.map((k) => k.title)).toEqual(["People", "Emails", "Phone numbers", "Private links"]);
  const g = find("Write to a@b.co about $5k, see https://x.io/a and http://localhost:8080/x.");
  expect(byKind(g).emails).toEqual(["a@b.co"]);
  expect(byKind(g).links).toEqual(["http://localhost:8080/x"]);
  expect(nonEmpty(g).map((x) => x.title)).toEqual(["Emails", "Private links"]);
  expect(nonEmpty(find("Nothing here."))).toEqual([]);
});

test("not flagged: capitalised words, handles, money, files, public links", () => {
  const text = "## In short\nUsed GitHub, the Blog and a Post on Medium. Ping @devraj. Paid $2 million. "
    + "Saved /usr/local/bin/x. Read https://saachis-blog.hashnode.dev/post and https://github.com/a/b?tab=readme.\n";
  expect(nonEmpty(find(text))).toEqual([]);
});

test("People section: names flagged, things not", () => {
  const text = "## People, links and files\n- GitHub\n- Blog\n- Post\n- API docs\n- the team\n- Bob 2\n"
    + "- **Ana**: design\n- Ana Ruiz — client\n- Seán O'Neil (lawyer)\n- Mary-Jane McKay, PM\n- Ana Lima <ana@x.com>\n";
  expect(byKind(find(text)).people).toEqual(["Ana", "Ana Ruiz", "Seán O'Neil", "Mary-Jane McKay", "Ana Lima"]);
  expect(byKind(find("x\n## Notes\n- Ana Lima\n")).people).toEqual([]); // only in the People section
});

test("private links", () => {
  for (const u of ["http://localhost:3000", "http://intranet/home", "https://a.b.com/x?api_key=1", "https://u:p@site.com/",
    "https://docs.google.com/spreadsheets/d/abc/edit", "https://www.figma.com/design/pricing-v2", "https://www.dropbox.com/s/x/y.pdf",
    "https://x.com/r?ref=Ab12Cd34Ef56Gh78Ij90"]) expect(isPrivateLink(u)).toBe(true);
  for (const u of ["https://saachis-blog.hashnode.dev/", "https://github.com/Saachi26/ML-Journey", "https://docs.google.com/",
    "https://www.youtube.com/watch?v=dQw4w9WgXcQ", "https://example.com/search?q=logistic+loss", "https://figma.com/community"]) {
    expect(isPrivateLink(u)).toBe(false);
  }
});

test("front matter: skipped by find (CRLF too), changed by remove", () => {
  expect(body("---\na: b\n---\nbody")).toBe("\nbody"); // as the Mac: from the end of the closing ---
  expect(body("---\r\na: b\r\n---\r\nbody")).toBe("\nbody");
  expect(body("no front matter")).toBe("no front matter");
  const text = "---\ntitle: Re: Ana\n---\n\nAna said re.\n";
  expect(remove(text, ["re", "Ana"])).toBe("---\ntitle: Re: [removed]\n---\n\n[removed] said [removed].\n");
  expect(remove("removed re", ["re", "removed"])).toBe("[removed] [removed]");
});
