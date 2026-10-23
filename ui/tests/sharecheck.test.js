// context-capsule — ui: tests for sharing a capsule (every case in spec/share-check.json, plus edges and real capsules)
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { find, findings, segments, shareCopy, remove, body, nonEmpty, KINDS, isPrivateLink } from "../js/core/sharecheck.js";

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

cases.findings.forEach((c) => {
  test(`findings: ${c.name}`, () => {
    expect(findings(c.text)).toEqual(c.expect);
  });
});

test("kinds in order with their titles and what is hidden at first", () => {
  expect(KINDS.map((k) => k.title)).toEqual(["Names", "Emails", "Phone numbers", "Private links", "Money", "Companies", "File paths", "Public links"]);
  expect(KINDS.filter((k) => k.on).map((k) => k.kind)).toEqual(["people", "emails", "phones", "links", "money"]);
  const g = find("Write to a@b.co about $5k, see https://x.io/a and http://localhost:8080/x.");
  expect(byKind(g).emails).toEqual(["a@b.co"]);
  expect(byKind(g).links).toEqual(["http://localhost:8080/x"]);
  expect(byKind(g).public).toEqual(["https://x.io/a"]);
  expect(nonEmpty(g).map((x) => x.title)).toEqual(["Emails", "Private links", "Money", "Public links"]);
  expect(nonEmpty(find("Nothing here."))).toEqual([]);
});

test("not flagged: capitalised words, handles, percentages, numbers, other paths", () => {
  const text = "## In short\nUsed GitHub, the Blog and a Post on Medium. Ping @devraj. Revenue up 18%, churn 3.1%, 95.19% accuracy. "
    + "Saved /usr/local/bin/x and plain/. Version 2.3.1, 40 seats, fixes #2–10, commits `5eeb26b8a..9c4792a98`. Q3 is done.\n";
  expect(nonEmpty(find(text))).toEqual([]);
});

test("money: signs, codes and words; not percentages or bare numbers", () => {
  const m = (t) => byKind(find(t)).money;
  expect(m("Paid $2 million and £30, ¥500, ₹4,500, INR 4,500, 300 rupees, 12k USD.")).toEqual(["$2 million", "£30", "¥500", "₹4,500", "INR 4,500", "300 rupees", "12k USD"]);
  expect(m("Teams $12 a seat, $3k a month")).toEqual(["$12", "$3k"]);
  expect(m("18% more, 2026-10-19, 4500 people, USDC, 5$")).toEqual([]);
});

test("companies: conservative", () => {
  const c = (t) => byKind(find(t)).companies;
  expect(c("We met Acme Corp. and the company Globex; Initech, the client, agreed.")).toEqual(["Acme Corp.", "Acme", "Globex"]);
  expect(c("Drafted Acme Ltd terms.")).toEqual(["Acme Ltd", "Acme"]); // a sentence's first word isn't part of it
  expect(c("Used GitHub for the client. The company website. Our client GitHub. Blog Post Inc")).toEqual([]);
  expect(c("## People, links and files\n- Ana Ruiz — designer from Hooli\n- Umbrella — client company\n- GitHub profile at GitHub\n"))
    .toEqual(["Hooli", "Umbrella"]);
  expect(byKind(find("## People\n- Umbrella — client company\n")).people).toEqual([]); // an organisation, not a person
});

test("file paths", () => {
  const f = (t) => byKind(find(t)).files;
  expect(f("Open ~/Documents/a b.md, /Users/ana/x.txt; /home/ana/y and C:\\Users\\ana\\z.docx.")).toEqual(["~/Documents/a", "/Users/ana/x.txt", "/home/ana/y", "C:\\Users\\ana\\z.docx"]);
  expect(f("Not: /usr/bin, a/b, https://x.io/Users/a, ana@x.co, 1/2")).toEqual([]);
  // a path's digits are not a phone number
  expect(byKind(find("See ~/sessions/20261019-145700/events.jsonl")).phones).toEqual([]);
});

test("given names on their own, only for People names", () => {
  const text = "Ana said yes. Will Smith may come. Analysis done.\n\n## People, links and files\n- Ana Ruiz — client\n- Will Smith\n";
  expect(byKind(find(text)).people).toEqual(["Ana", "Will Smith", "Ana Ruiz"]); // not "Will": an ordinary word
  expect(byKind(find("## People\n- Ana Ruiz\n")).people).toEqual(["Ana Ruiz"]);
});

test("whole words only: Ana is not inside Analysis", () => {
  expect(remove("Ana did the Analysis with Anand.", ["Ana"])).toBe("[removed] did the Analysis with Anand.");
  expect(remove("$12 and $12,000", ["$12", "$12,000"])).toBe("[removed] and [removed]");
});

test("the copy: only hidden items, as the page shows them", () => {
  const text = "---\ntitle: Call Ana Ruiz\n---\n\nAna Ruiz at Acme, $500 on https://acme.com/Ana Ruiz.\n";
  const items = ["Ana Ruiz", "Acme", "$500"];
  expect(shareCopy(text, items, new Set(["Ana Ruiz", "$500"]))).toBe("---\ntitle: Call [removed]\n---\n\n[removed] at Acme, [removed] on https://acme.com/[removed].\n");
  expect(shareCopy(text, items, [])).toBe(text);
  // a hidden item inside a longer kept one stays (the page shows the longer one)
  expect(shareCopy("Acme Inc and Acme", ["Acme Inc", "Acme"], ["Acme"])).toBe("Acme Inc and [removed]");
  expect(segments("Hi Ana.", ["Ana"])).toEqual([{ text: "Hi " }, { text: "Ana", item: "Ana" }, { text: "." }]);
});

// Real capsules: nothing that is hidden at first may be wrong. Read only, never changed.
const real = [join(homedir(), ".context-capsule", "capsules"), "/tmp/mac-ref-data/capsules"].filter((d) => existsSync(d));
test.skipIf(!real.length)("real capsules: no false names, emails, phones, private links, money or companies", () => {
  const allowed = {
    money: new Set(["$12", "$24"]), // the seeded pricing capsules
    links: new Set(["https://www.figma.com/design/pricing-v2"]),
  };
  for (const dir of real) {
    for (const f of readdirSync(dir).filter((n) => n.endsWith(".md"))) {
      const g = byKind(find(readFileSync(join(dir, f), "utf8")));
      for (const k of ["people", "emails", "phones", "links", "money", "companies"]) {
        const wrong = g[k].filter((x) => !allowed[k]?.has(x));
        expect({ file: f, kind: k, wrong }).toEqual({ file: f, kind: k, wrong: [] });
      }
      for (const p of g.files) expect(p).toMatch(/^(?:~|\/Users|\/home|[A-Za-z]:)[\\/]/);
      for (const u of g.public) expect(isPrivateLink(u)).toBe(false);
    }
  }
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
