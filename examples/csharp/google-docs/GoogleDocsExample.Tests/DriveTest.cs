using System.Net;
using System.Net.Http.Json;
using Authzed.Api.V1;
using Borkfork.SpiceDb.Examples.GoogleDocs;
using Xunit;

namespace GoogleDocsExample.Tests;

public class DriveTest
{
    private static Relationship Rel(string resource, string relation, string subject)
    {
        var resParts = resource.Split(':');
        var subParts = subject.Split(':');
        return new Relationship
        {
            Resource = new ObjectReference { ObjectType = resParts[0], ObjectId = resParts[1] },
            Relation = relation,
            Subject = new SubjectReference
            {
                Object = new ObjectReference { ObjectType = subParts[0], ObjectId = subParts[1] }
            }
        };
    }

    [Fact]
    public async Task Grants_alice_viewer_on_folder_and_verifies_view_on_documents()
    {
        var (app, spicedb) = Server.CreateApp(Array.Empty<Relationship>());
        using (spicedb)
        {
            _ = spicedb.Permissions().WriteRelationships(new WriteRelationshipsRequest
            {
                Updates =
                {
                    new RelationshipUpdate { Operation = RelationshipUpdate.Types.Operation.Touch, Relationship = Rel("document:doc1", "folder", "folder:marketing") },
                    new RelationshipUpdate { Operation = RelationshipUpdate.Types.Operation.Touch, Relationship = Rel("document:doc2", "folder", "folder:marketing") },
                }
            });
            _ = spicedb.Permissions().WriteRelationships(new WriteRelationshipsRequest
            {
                Updates = { new RelationshipUpdate { Operation = RelationshipUpdate.Types.Operation.Touch, Relationship = Rel("folder:marketing", "viewer", "user:alice") } }
            });

            var port = 30000 + Random.Shared.Next(0, 10000);
            app.Urls.Add($"http://127.0.0.1:{port}");
            await app.StartAsync();
            try
            {
                using var client = new HttpClient { BaseAddress = new Uri($"http://127.0.0.1:{port}") };
                client.DefaultRequestHeaders.Add("X-User", "alice");

                var res1 = await client.GetAsync("/drive/marketing/doc1");
                var res2 = await client.GetAsync("/drive/marketing/doc2");

                Assert.Equal(HttpStatusCode.OK, res1.StatusCode);
                var body1 = await res1.Content.ReadFromJsonAsync<DriveResponse>();
                Assert.NotNull(body1);
                Assert.True(body1.Ok);
                Assert.Equal("alice", body1.User);
                Assert.Equal("doc1", body1.Document);

                Assert.Equal(HttpStatusCode.OK, res2.StatusCode);
                var body2 = await res2.Content.ReadFromJsonAsync<DriveResponse>();
                Assert.NotNull(body2);
                Assert.True(body2.Ok);
                Assert.Equal("doc2", body2.Document);
            }
            finally
            {
                await app.StopAsync();
            }
        }
    }

    [Fact]
    public async Task Denies_bob_when_not_viewer()
    {
        var (app, spicedb) = Server.CreateApp(new[]
        {
            Rel("document:doc1", "folder", "folder:marketing"),
            Rel("folder:marketing", "viewer", "user:alice")
        });
        using (spicedb)
        {
            var port = 30000 + Random.Shared.Next(0, 10000);
            app.Urls.Add($"http://127.0.0.1:{port}");
            await app.StartAsync();
            try
            {
                using var client = new HttpClient { BaseAddress = new Uri($"http://127.0.0.1:{port}") };
                client.DefaultRequestHeaders.Add("X-User", "bob");

                var res = await client.GetAsync("/drive/marketing/doc1");

                Assert.Equal(HttpStatusCode.Forbidden, res.StatusCode);
                var body = await res.Content.ReadFromJsonAsync<ErrorResponse>();
                Assert.NotNull(body);
                Assert.Equal("Forbidden", body.Error);
            }
            finally
            {
                await app.StopAsync();
            }
        }
    }

    [Fact]
    public async Task Inherits_view_from_parent_folder()
    {
        var (app, spicedb) = Server.CreateApp(new[]
        {
            Rel("folder:marketing", "parent", "folder:root"),
            Rel("document:doc1", "folder", "folder:marketing"),
            Rel("folder:root", "viewer", "user:alice")
        });
        using (spicedb)
        {
            var port = 30000 + Random.Shared.Next(0, 10000);
            app.Urls.Add($"http://127.0.0.1:{port}");
            await app.StartAsync();
            try
            {
                using var client = new HttpClient { BaseAddress = new Uri($"http://127.0.0.1:{port}") };
                client.DefaultRequestHeaders.Add("X-User", "alice");

                var res = await client.GetAsync("/drive/marketing/doc1");

                Assert.Equal(HttpStatusCode.OK, res.StatusCode);
                var body = await res.Content.ReadFromJsonAsync<DriveResponse>();
                Assert.NotNull(body);
                Assert.True(body.Ok);
                Assert.Equal("alice", body.User);
                Assert.Equal("doc1", body.Document);
            }
            finally
            {
                await app.StopAsync();
            }
        }
    }

    [Fact]
    public async Task User_can_be_granted_permission_directly_on_document()
    {
        var (app, spicedb) = Server.CreateApp(new[]
        {
            Rel("document:doc1", "folder", "folder:marketing"),
            Rel("document:doc1", "viewer", "user:alice")
        });
        using (spicedb)
        {
            var port = 30000 + Random.Shared.Next(0, 10000);
            app.Urls.Add($"http://127.0.0.1:{port}");
            await app.StartAsync();
            try
            {
                using var client = new HttpClient { BaseAddress = new Uri($"http://127.0.0.1:{port}") };
                client.DefaultRequestHeaders.Add("X-User", "alice");

                var res = await client.GetAsync("/drive/marketing/doc1");

                Assert.Equal(HttpStatusCode.OK, res.StatusCode);
                var body = await res.Content.ReadFromJsonAsync<DriveResponse>();
                Assert.NotNull(body);
                Assert.True(body.Ok);
                Assert.Equal("doc1", body.Document);
            }
            finally
            {
                await app.StopAsync();
            }
        }
    }

    [Fact]
    public async Task Returns_401_when_X_User_header_missing()
    {
        var (app, spicedb) = Server.CreateApp(new[]
        {
            Rel("document:doc1", "folder", "folder:marketing"),
            Rel("folder:marketing", "viewer", "user:alice")
        });
        using (spicedb)
        {
            var port = 30000 + Random.Shared.Next(0, 10000);
            app.Urls.Add($"http://127.0.0.1:{port}");
            await app.StartAsync();
            try
            {
                using var client = new HttpClient { BaseAddress = new Uri($"http://127.0.0.1:{port}") };

                var res = await client.GetAsync("/drive/marketing/doc1");

                Assert.Equal(HttpStatusCode.Unauthorized, res.StatusCode);
                var body = await res.Content.ReadFromJsonAsync<ErrorResponse>();
                Assert.NotNull(body);
                Assert.Equal("Missing X-User header", body.Error);
            }
            finally
            {
                await app.StopAsync();
            }
        }
    }

    private sealed class DriveResponse
    {
        public bool Ok { get; set; }
        public string? User { get; set; }
        public string? Folder { get; set; }
        public string? Document { get; set; }
        public string? Message { get; set; }
    }

    private sealed class ErrorResponse
    {
        public string? Error { get; set; }
        public string? Message { get; set; }
    }
}
