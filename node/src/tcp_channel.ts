/**
 * gRPC target for TCP connections.
 */

/** Returns the gRPC target string for a TCP connection (e.g. 127.0.0.1:50051). */
export function getTarget(address: string): string {
  return address;
}
