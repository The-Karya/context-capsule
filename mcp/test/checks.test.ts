// context-capsule — mcp: tests for the capsule checks (refuse incomplete capsules with precise fixes)
import { describe, expect, test } from "bun:test";
import { checkCapsule, listItems, sectionsOf } from "../src/checks.ts";
import { VALID_BODY } from "./helpers.ts";

const swap = (heading: string, body: string) =>
  VALID_BODY.replace(new RegExp(`(## ${heading}\\n)[\\s\\S]*?(?=\\n## |$)`), `$1${body}`);

describe("capsule checks", () => {
  test("a complete capsule passes", () => {
    expect(checkCapsule(VALID_BODY)).toEqual({ errors: [], warnings: [] });
  });

  test("sections and list items are parsed", () => {
    const s = sectionsOf("intro\n## A:\none\n## B ##\n- x\n  wrapped\n2. y\n");
    expect([...s.keys()]).toEqual(["a", "b"]);
    expect(listItems(s.get("b")!)).toEqual(["x wrapped", "y"]);
  });

  test("missing sections are named", () => {
    const r = checkCapsule("## In short\nHi there.\n");
    expect(r.errors).toContain('Add the section "## For Claude".');
    expect(r.errors).toContain('Add the section "## People, links and files".');
  });

  test("For Claude must really speak to Claude", () => {
    const r = checkCapsule(swap("For Claude", "Pick this up."));
    expect(r.errors.join(" ")).toContain('"## For Claude" is too short');
  });

  test("every decision says where it came from", () => {
    const r = checkCapsule(swap("Decisions", "- **Ship Monday** — ready.\n- **Drop Starter** — nobody buys it. *(you said, 10:14)*\n- **Price** — guessed. *(guess)*"));
    expect(r.errors.length).toBe(1);
    expect(r.errors[0]).toContain('"**Ship Monday** — ready."');
    expect(r.errors[0]).not.toContain("Drop Starter");
    expect(checkCapsule(swap("Decisions", "None")).errors).toEqual([]);
    expect(checkCapsule(swap("Decisions", "We talked a lot.")).errors.join(" ")).toContain('or write "None"');
  });

  test("empty In short / Next steps are errors; untimed work and long text are warnings", () => {
    expect(checkCapsule(swap("In short", "")).errors.join(" ")).toContain('"## In short" is empty');
    expect(checkCapsule(swap("Next steps", "")).errors.join(" ")).toContain('"## Next steps" is empty');
    const w = checkCapsule(swap("What was done", "- Did things\n- 10:05 More things")).warnings;
    expect(w.join(" ")).toContain("1 item(s) have no time");
    expect(checkCapsule(VALID_BODY + "\nword ".repeat(3100)).warnings.join(" ")).toContain("aim for 2500");
  });
});
