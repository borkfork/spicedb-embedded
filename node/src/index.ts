/**
 * Embedded SpiceDB for TypeScript/Node.js — in-memory authorization server for tests and development.
 */

export { EmbeddedSpiceDB, SpiceDBError } from "./embedded.js";
export type { SpiceDBStartResult } from "./ffi.js";

// Re-export authzed v1 for types and client creation
export { v1 } from "@authzed/authzed-node";
