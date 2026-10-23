// context-capsule — ui: tests for projects — slug, projects.json in and out, clues, edits.

import { expect, test } from "bun:test";
import * as P from "../js/core/projects.js";

test("slug", () => {
  expect(P.slug("Client Report (Q3)!")).toBe("client-report-q3");
  expect(P.slug("Café Déjà vu")).toBe("cafe-deja-vu");
  expect(P.slug("  --Hello--  ")).toBe("hello");
  expect(P.slug("ＦＵＬＬ width")).toBe("full-width");
  expect(P.slug("a".repeat(59) + " b")).toBe("a".repeat(59));
  expect(P.slug("日本")).toBe("");
});

test("parse", () => {
  const list = P.parse(`{"projects":[{"id":"web","name":" Website ","match":[" linear.app ",""]},{"name":"Client Report"},{"name":"  "},{"id":"x","name":"X","match":[1]}]}`);
  expect(list).toEqual([
    { id: "web", name: "Website", match: ["linear.app"] },
    { id: "client-report", name: "Client Report", match: [] },
    { id: "x", name: "X", match: [] },
  ]);
  expect(P.parse("nope")).toEqual([]);
  expect(P.parse(`{"projects":[{"name":"a"},3]}`)).toEqual([]);
});

test("detect", () => {
  const ps = [
    { id: "a", name: "A", match: ["acme", "x"] },
    { id: "b", name: "B", match: ["acme q3"] },
    { id: "c", name: "C", match: ["ACME"] },
  ];
  expect(P.detect(ps, { title: "Acme Q3 report" })?.id).toBe("b");
  expect(P.detect(ps, { title: "Acme" })?.id).toBe("a");
  expect(P.detect(ps, { where: "https://acme.com" })?.id).toBe("a");
  expect(P.detect(ps, { app: "ACMEapp" })?.id).toBe("a");
  expect(P.detect(ps, { title: "x" })).toBeNull();
  expect(P.detect([], { title: "acme" })).toBeNull();
  expect(P.nameOf(ps, "b")).toBe("B");
  expect(P.nameOf(ps, "zz")).toBe("zz");
});

test("edits", () => {
  const ps = [{ id: "a", name: "A", match: [] }, { id: "b", name: "B", match: ["bee"] }];
  expect(P.rename(ps, "a", "  New ")[0].name).toBe("New");
  expect(P.rename(ps, "a", "  ")).toBe(ps);
  expect(P.setClues(" acme , q3 report, x,,")).toEqual(["acme", "q3 report"]);
  expect(P.withClues(ps, "b", "one, two")[1].match).toEqual(["one", "two"]);
  expect(P.remove(ps, "a")).toEqual([ps[1]]);
  expect(P.create(ps, "Client report", "acme")).toEqual([...ps, { id: "client-report", name: "Client report", match: ["acme"] }]);
  expect(P.create(ps, "b", "BEE")).toBe(ps);
  expect(P.create(ps, "B", "hive")[1].match).toEqual(["bee", "hive"]);
  expect(P.create(ps, "!!")).toBeNull();
});

test("toJSON as the Mac writes it", () => {
  expect(P.toJSON([])).toBe('{\n  "projects" : [\n\n  ]\n}\n');
  const text = P.toJSON([
    { id: "web", name: "Web/site", match: ["linear.app", 'a"b'] },
    { id: "x", name: "X", match: [] },
  ]);
  expect(text).toBe(
    '{\n  "projects" : [\n    {\n      "id" : "web",\n      "match" : [\n        "linear.app",\n        "a\\"b"\n      ],\n      "name" : "Web\\/site"\n    },\n' +
      '    {\n      "id" : "x",\n      "match" : [\n\n      ],\n      "name" : "X"\n    }\n  ]\n}\n',
  );
  expect(P.parse(text)).toEqual([
    { id: "web", name: "Web/site", match: ["linear.app", 'a"b'] },
    { id: "x", name: "X", match: [] },
  ]);
});
