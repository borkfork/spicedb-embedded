"""gRPC target for Unix domain socket connections."""


def get_target(address: str) -> str:
    """Return the gRPC target string for a Unix domain socket (e.g. unix:///tmp/spicedb-123.sock)."""
    return f"unix://{address}"
