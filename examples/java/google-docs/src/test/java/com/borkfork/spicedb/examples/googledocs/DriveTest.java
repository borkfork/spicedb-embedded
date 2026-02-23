package com.borkfork.spicedb.examples.googledocs;

import static org.junit.jupiter.api.Assertions.*;

import com.google.gson.Gson;
import com.google.gson.JsonObject;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.List;
import org.junit.jupiter.api.Test;

class DriveTest {

  private static HttpResponse<String> get(
      int port, String path, String xUser) throws Exception {
    var client = HttpClient.newHttpClient();
    var builder =
        HttpRequest.newBuilder()
            .uri(URI.create("http://127.0.0.1:" + port + path))
            .GET();
    if (xUser != null) {
      builder.header("X-User", xUser);
    }
    return client.send(builder.build(), HttpResponse.BodyHandlers.ofString());
  }

  private static JsonObject json(String body) {
    return new Gson().fromJson(body, JsonObject.class);
  }

  @Test
  void aliceViewerCanViewDocuments() throws Exception {
    var appAndDb =
        Server.createApp(
            List.of(
                Server.rel("document:doc1", "folder", "folder:marketing"),
                Server.rel("document:doc2", "folder", "folder:marketing"),
                Server.rel("folder:marketing", "viewer", "user:alice")));

    appAndDb.app().start(0);
    int port = appAndDb.app().port();

    try {
      var r1 = get(port, "/drive/marketing/doc1", "alice");
      var r2 = get(port, "/drive/marketing/doc2", "alice");

      assertEquals(200, r1.statusCode());
      assertTrue(json(r1.body()).get("ok").getAsBoolean());
      assertEquals("alice", json(r1.body()).get("user").getAsString());
      assertEquals("doc1", json(r1.body()).get("document").getAsString());
      assertEquals(200, r2.statusCode());
      assertEquals("doc2", json(r2.body()).get("document").getAsString());
    } finally {
      appAndDb.app().stop();
      appAndDb.spicedb().close();
    }
  }

  @Test
  void bobDeniedWhenNotViewer() throws Exception {
    var appAndDb =
        Server.createApp(
            List.of(
                Server.rel("document:doc1", "folder", "folder:marketing"),
                Server.rel("folder:marketing", "viewer", "user:alice")));

    appAndDb.app().start(0);
    int port = appAndDb.app().port();

    try {
      var r = get(port, "/drive/marketing/doc1", "bob");
      assertEquals(403, r.statusCode());
      assertEquals("Forbidden", json(r.body()).get("error").getAsString());
    } finally {
      appAndDb.app().stop();
      appAndDb.spicedb().close();
    }
  }

  @Test
  void inheritsViewFromParentFolder() throws Exception {
    var appAndDb =
        Server.createApp(
            List.of(
                Server.rel("folder:marketing", "parent", "folder:root"),
                Server.rel("document:doc1", "folder", "folder:marketing"),
                Server.rel("folder:root", "viewer", "user:alice")));

    appAndDb.app().start(0);
    int port = appAndDb.app().port();

    try {
      var r = get(port, "/drive/marketing/doc1", "alice");
      assertEquals(200, r.statusCode());
      assertTrue(json(r.body()).get("ok").getAsBoolean());
    } finally {
      appAndDb.app().stop();
      appAndDb.spicedb().close();
    }
  }

  @Test
  void returns401WhenXUserMissing() throws Exception {
    var appAndDb =
        Server.createApp(
            List.of(
                Server.rel("document:doc1", "folder", "folder:marketing"),
                Server.rel("folder:marketing", "viewer", "user:alice")));

    appAndDb.app().start(0);
    int port = appAndDb.app().port();

    try {
      var r = get(port, "/drive/marketing/doc1", null);
      assertEquals(401, r.statusCode());
      assertEquals("Missing X-User header", json(r.body()).get("error").getAsString());
    } finally {
      appAndDb.app().stop();
      appAndDb.spicedb().close();
    }
  }
}
