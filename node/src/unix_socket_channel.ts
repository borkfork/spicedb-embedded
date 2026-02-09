/**
 * gRPC target for Unix domain socket connections.
 */

/** Returns the gRPC target string for a Unix domain socket (e.g. unix:///tmp/spicedb-123.sock). */
export function getTarget(address: string): string {
  return `unix://${address}`;
}
