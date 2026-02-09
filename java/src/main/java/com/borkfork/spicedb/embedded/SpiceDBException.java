package com.borkfork.spicedb.embedded;

/** Thrown when SpiceDB operations fail. */
public class SpiceDBException extends RuntimeException {

  public SpiceDBException(String message) {
    super(message);
  }

  public SpiceDBException(String message, Throwable cause) {
    super(message, cause);
  }
}
