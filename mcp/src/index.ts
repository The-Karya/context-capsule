#!/usr/bin/env node
// context-capsule — mcp: entry point of context-capsule-mcp: the MCP server (stdio by default) and the app commands
import { main } from "./cli.js";

main(process.argv.slice(2)).catch((e) => {
  console.error("context-capsule-mcp failed:", e instanceof Error ? e.message : e);
  process.exit(1);
});
