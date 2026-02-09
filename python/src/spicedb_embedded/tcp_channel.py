"""gRPC target for TCP connections."""


def get_target(address: str) -> str:
    """Return the gRPC target string for a TCP connection (e.g. 127.0.0.1:50051)."""
    return address
