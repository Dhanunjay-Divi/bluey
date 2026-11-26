// Detached nonproduction MCP stdio fixture. No HTTP, credentials, files,
// real Jobs reader, cursor authority or write tools.
// Base SHA-256: 36618d716c6039f4c0cc4d27fac53aa2a6a6920f788b09a1b49ce9ad120867ea
import { once } from "node:events";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const VERSION = "2025-11-25";
const MAX_FRAME_BYTES = 8192;
const MAX_SESSION_BYTES = 256 * 1024;
const MAX_SESSION_FRAMES = 256;
const CURSOR = /^bjmcp1_[0-9a-f]{64}$/;
const JOB_MATCHES = Object.freeze({
  "track-alpha": Object.freeze([
    Object.freeze({ jobId: "fixture-alpha-1", title: "Data Engineer", company: "Synthetic Alpha" }),
    Object.freeze({ jobId: "fixture-alpha-2", title: "Analytics Engineer", company: "Synthetic Alpha" }),
  ]),
  "track-beta": Object.freeze([
    Object.freeze({ jobId: "fixture-beta-1", title: "Platform Engineer", company: "Synthetic Beta" }),
  ]),
});

const MATCH = Object.freeze({
  type: "object",
  required: ["jobId", "title", "company"],
  additionalProperties: false,
  properties: {
    jobId: { type: "string", minLength: 1, maxLength: 128 },
    title: { type: "string", minLength: 1, maxLength: 1024 },
    company: { type: "string", minLength: 1, maxLength: 768 },
  },
});

const TOOL = Object.freeze({
  name: "get_job_matches",
  title: "Read synthetic Jobs matches",
  description: "Reads one fixture-only shortlist page for the process-selected Career Track. This simulator does not implement or qualify a real continuation cursor.",
  inputSchema: {
    oneOf: [
      {
        type: "object",
        additionalProperties: false,
        properties: { limit: { type: "integer", minimum: 1, maximum: 10 } },
      },
      {
        type: "object",
        required: ["cursor"],
        additionalProperties: false,
        properties: {
          cursor: { type: "string", pattern: "^bjmcp1_[0-9a-f]{64}$" },
        },
      },
    ],
  },
  outputSchema: {
    type: "object",
    required: ["fixtureOnly", "trackId", "matches", "nextCursor", "shortlistTruncated"],
    additionalProperties: false,
    properties: {
      fixtureOnly: { type: "boolean", const: true },
      trackId: { type: "string", minLength: 1, maxLength: 64 },
      matches: { type: "array", maxItems: 10, items: MATCH },
      nextCursor: { oneOf: [
        { type: "null" },
        { type: "string", minLength: 71, maxLength: 71, pattern: "^bjmcp1_[0-9a-f]{64}$" },
      ] },
      shortlistTruncated: { type: "boolean" },
    },
  },
  annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
});

const object = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const only = (value, keys) => object(value) && Object.keys(value).every((key) => keys.includes(key));
const boundedString = (value, length) => typeof value === "string" && value.length > 0 && value.length <= length;
const validId = (id) => boundedString(id, 128) || Number.isSafeInteger(id);
const success = (id, result) => ({ jsonrpc: "2.0", id, result });
const error = (id, code, message) => ({ jsonrpc: "2.0", id, error: { code, message } });
const noParams = (value) => value === undefined || only(value, []);
const unavailable = () => ({
  isError: true,
  content: [{ type: "text", text: "Career Track unavailable" }],
});

function parseArguments(args) {
  if (!object(args)) return undefined;
  if (only(args, ["limit"]) &&
      (args.limit === undefined || (Number.isSafeInteger(args.limit) && args.limit >= 1 && args.limit <= 10))) {
    return { kind: "start", limit: args.limit ?? 10 };
  }
  if (only(args, ["cursor"]) && typeof args.cursor === "string" && CURSOR.test(args.cursor)) {
    return { kind: "cursor" };
  }
  return undefined;
}

export function createFixtureMcpSession(account) {
  if (account !== "alpha" && account !== "beta") throw Error("invalid_fixture_account");
  const ownerTrack = `track-${account}`;
  let phase = "new";
  return (message) => {
    if (!only(message, ["jsonrpc", "id", "method", "params"]) || message.jsonrpc !== "2.0" ||
      !boundedString(message.method, 128) || (Object.hasOwn(message, "id") && !validId(message.id))) {
      return error(null, -32600, "Invalid request");
    }
    const { id, method, params } = message;
    if (id === undefined) {
      if (method === "notifications/initialized" && phase === "initializing" && noParams(params)) phase = "ready";
      return undefined;
    }
    if (method === "ping" && noParams(params)) return success(id, {});
    if (method === "initialize") {
      if (phase !== "new") return error(id, -32600, "Session already initialized");
      if (!only(params, ["protocolVersion", "capabilities", "clientInfo"]) ||
        !boundedString(params.protocolVersion, 32) || !object(params.capabilities) ||
        !object(params.clientInfo) || !boundedString(params.clientInfo.name, 128) ||
        !boundedString(params.clientInfo.version, 128)) return error(id, -32602, "Invalid initialize parameters");
      phase = "initializing";
      return success(id, {
        protocolVersion: VERSION,
        capabilities: { tools: { listChanged: false } },
        serverInfo: { name: "bluey-jobs-synthetic", version: "1.0.0" },
      });
    }
    if (phase !== "ready") return error(id, -32002, "Session is not initialized");
    if (method === "tools/list") {
      return noParams(params) ? success(id, { tools: [structuredClone(TOOL)] }) :
        error(id, -32602, "Invalid list parameters");
    }
    if (method !== "tools/call") return error(id, -32601, "Method not found");
    if (!only(params, ["name", "arguments"]) || params.name !== TOOL.name) {
      return error(id, -32602, "Unknown tool or invalid parameters");
    }
    const parsed = parseArguments(params.arguments);
    if (!parsed) return error(id, -32602, "Invalid match parameters");
    if (parsed.kind === "cursor") return success(id, unavailable());
    const result = {
      fixtureOnly: true,
      trackId: ownerTrack,
      matches: JOB_MATCHES[ownerTrack].slice(0, parsed.limit),
      nextCursor: null,
      shortlistTruncated: false,
    };
    return success(id, {
      isError: false,
      structuredContent: result,
      content: [{ type: "text", text: JSON.stringify(result) }],
    });
  };
}

async function runStdio(handler) {
  const utf8 = new TextDecoder("utf-8", { fatal: true });
  let pending = Buffer.alloc(0);
  let oversized = false;
  let sessionBytes = 0;
  let frames = 0;
  const write = async (response) => {
    if (response && !process.stdout.write(`${JSON.stringify(response)}\n`)) await once(process.stdout, "drain");
  };
  for await (const chunk of process.stdin) {
    sessionBytes += chunk.length;
    if (sessionBytes > MAX_SESSION_BYTES) {
      await write(error(null, -32000, "Fixture input limit reached"));
      return;
    }
    let start = 0;
    while (start < chunk.length) {
      const end = chunk.indexOf(10, start);
      const part = chunk.subarray(start, end === -1 ? chunk.length : end);
      if (!oversized && part.length + pending.length <= MAX_FRAME_BYTES) pending = Buffer.concat([pending, part]);
      else { pending = Buffer.alloc(0); oversized = true; }
      if (end === -1) break;
      if (++frames > MAX_SESSION_FRAMES) {
        await write(error(null, -32000, "Fixture frame limit reached"));
        return;
      }
      if (oversized) await write(error(null, -32700, "Frame exceeds limit"));
      else {
        let message;
        try { message = JSON.parse(utf8.decode(pending)); }
        catch { await write(error(null, -32700, "Unable to parse frame")); }
        if (message !== undefined) await write(handler(message));
      }
      pending = Buffer.alloc(0);
      oversized = false;
      start = end + 1;
    }
  }
  if (oversized || pending.length) await write(error(null, -32700, "Incomplete frame"));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const argument = process.argv[2];
  if (process.argv.length !== 3 || !["--synthetic-account=alpha", "--synthetic-account=beta"].includes(argument)) {
    process.exitCode = 64;
  } else {
    try { await runStdio(createFixtureMcpSession(argument.slice("--synthetic-account=".length))); }
    catch { process.exitCode = 1; }
  }
}
