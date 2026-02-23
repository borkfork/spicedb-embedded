package com.borkfork.spicedb.examples.googledocs;

/** Run the Google Docs example server on port 3000. */
public final class Main {

  public static void main(String[] args) {
    var appAndDb = Server.createApp(null);
    appAndDb.app().start(3000);
    System.err.println("Server listening on http://localhost:3000");
    System.err.println(
        "Try: curl -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1");
  }
}
