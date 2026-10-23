// context-capsule — ui: tests for Check before sharing (every case in spec/share-check.json, plus edges)
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { find, remove, body, nonEmpty, KINDS } from "../js/core/sharecheck.js";

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
  expect(KINDS.map((k) => k.title)).toEqual(["People", "Emails", "Phone numbers", "Links", "Money", "Files"]);
  const g = find("Write to a@b.co about $5k and see https://x.io/a.");
  expect(byKind(g).emails).toEqual(["a@b.co"]);
  expect(byKind(g).money).toEqual(["$5k"]);
  expect(byKind(g).links).toEqual(["https://x.io/a"]);
  expect(nonEmpty(g).map((x) => x.title)).toEqual(["Emails", "Links", "Money"]);
  expect(nonEmpty(find("Nothing here."))).toEqual([]);
});

test("edges", () => {
  const f = byKind(find("Costs $1.5m, or 300 euros. Path /etc is short; /usr/local/bin is not. Call (020) 7946 0958 or 2026-10-19."));
  expect(f.money).toEqual(["$1.5m", "300 euros"]);
  expect(f.files).toEqual(["/usr/local/bin"]);
  expect(f.phones).toEqual(["020) 7946 0958"]);
  const g = byKind(find("Mail me@x.org, not @me.org. Ping @ana_b."));
  expect(g.emails).toEqual(["me@x.org"]);
  expect(g.people).toEqual(["@me", "@ana_b"]); // the Mac's handle rule: letters, digits, _
  expect(byKind(find("Raised $2 million.")).money).toEqual(["$2 million"]);
});

test("front matter: skipped by find (CRLF too), changed by remove", () => {
  expect(body("---\na: b\n---\nbody")).toBe("\nbody"); // as the Mac: from the end of the closing ---
  expect(body("---\r\na: b\r\n---\r\nbody")).toBe("\nbody");
  expect(body("no front matter")).toBe("no front matter");
  const text = "---\ntitle: Re: Ana\n---\n\nAna said re.\n";
  expect(remove(text, ["re", "Ana"])).toBe("---\ntitle: Re: [removed]\n---\n\n[removed] said [removed].\n");
  expect(remove("removed re", ["re", "removed"])).toBe("[removed] [removed]");
});
