#!/usr/bin/env node
// context-capsule — mcp: entry point, serves the MCP server over stdio
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { createServer } from "./server.js";

async function main(): Promise<void> {
  const server = createServer();
  await server.connect(new StdioServerTransport());
  // stdout is the protocol channel; logs go to stderr
  console.error("context-capsule MCP server running on stdio");
}

main().catch((e) => {
  console.error("context-capsule MCP server failed to start:", e);
  process.exit(1);
});
