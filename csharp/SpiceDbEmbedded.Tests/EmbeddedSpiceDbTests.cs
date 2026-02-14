using Authzed.Api.V1;
using Borkfork.SpiceDb.Embedded;
using Xunit;

namespace SpiceDbEmbedded.Tests;

public class EmbeddedSpiceDbTests
{
    private static readonly string TestSchema = """
                                                definition user {}

                                                definition document {
                                                    relation reader: user
                                                    relation writer: user

                                                    permission read = reader + writer
                                                    permission write = writer
                                                }
                                                """;

    private static Relationship Rel(string resource, string relation, string subject)
    {
        var (resType, resId) = (resource.Split(':')[0], resource.Split(':')[1]);
        var (subType, subId) = (subject.Split(':')[0], subject.Split(':')[1]);
        return new Relationship
        {
            Resource = new ObjectReference { ObjectType = resType, ObjectId = resId },
            Relation = relation,
            Subject = new SubjectReference { Object = new ObjectReference { ObjectType = subType, ObjectId = subId } }
        };
    }

    [Fact]
    public void CheckPermission()
    {
        var relationships = new[]
        {
            Rel("document:readme", "reader", "user:alice"),
            Rel("document:readme", "writer", "user:bob")
        };

        using var spicedb = EmbeddedSpiceDb.Create(TestSchema, relationships);

        var consistency = new Consistency { FullyConsistent = true };
        var checkReq = new CheckPermissionRequest
        {
            Consistency = consistency,
            Resource = new ObjectReference { ObjectType = "document", ObjectId = "readme" },
            Permission = "read",
            Subject = new SubjectReference { Object = new ObjectReference { ObjectType = "user", ObjectId = "alice" } }
        };

        var response = spicedb.Permissions().CheckPermission(checkReq);
        Assert.Equal(CheckPermissionResponse.Types.Permissionship.HasPermission, response.Permissionship);

        var writeReq = new CheckPermissionRequest
        {
            Consistency = consistency,
            Resource = checkReq.Resource,
            Permission = "write",
            Subject = checkReq.Subject
        };
        var writeResp = spicedb.Permissions().CheckPermission(writeReq);
        Assert.Equal(CheckPermissionResponse.Types.Permissionship.NoPermission, writeResp.Permissionship);

        var bobReadReq = new CheckPermissionRequest
        {
            Consistency = consistency,
            Resource = checkReq.Resource,
            Permission = "read",
            Subject = new SubjectReference { Object = new ObjectReference { ObjectType = "user", ObjectId = "bob" } }
        };
        var bobReadResp = spicedb.Permissions().CheckPermission(bobReadReq);
        Assert.Equal(CheckPermissionResponse.Types.Permissionship.HasPermission, bobReadResp.Permissionship);
    }

    [Fact]
    public async Task ReadRelationships_Streaming()
    {
        var relationships = new[]
        {
            Rel("document:doc1", "reader", "user:alice"),
            Rel("document:doc1", "writer", "user:bob")
        };
        using var spicedb = EmbeddedSpiceDb.Create(TestSchema, relationships);

        var req = new ReadRelationshipsRequest
        {
            RelationshipFilter = new RelationshipFilter
            {
                ResourceType = "document",
                OptionalResourceId = "doc1"
            }
        };
        var call = spicedb.Permissions().ReadRelationships(req);
        var results = new List<ReadRelationshipsResponse>();
        while (await call.ResponseStream.MoveNext(CancellationToken.None))
            results.Add(call.ResponseStream.Current);

        Assert.True(results.Count >= 2);
        Assert.All(results, r => Assert.Equal("document", r.Relationship?.Resource?.ObjectType));
    }

    [Fact]
    public void AddRelationship()
    {
        using var spicedb = EmbeddedSpiceDb.Create(TestSchema, []);

        _ = spicedb.Permissions().WriteRelationships(new WriteRelationshipsRequest
        {
            Updates =
            {
                new RelationshipUpdate
                {
                    Operation = RelationshipUpdate.Types.Operation.Touch,
                    Relationship = Rel("document:test", "reader", "user:alice")
                }
            }
        });

        var checkReq = new CheckPermissionRequest
        {
            Consistency = new Consistency { FullyConsistent = true },
            Resource = new ObjectReference { ObjectType = "document", ObjectId = "test" },
            Permission = "read",
            Subject = new SubjectReference { Object = new ObjectReference { ObjectType = "user", ObjectId = "alice" } }
        };
        var response = spicedb.Permissions().CheckPermission(checkReq);
        Assert.Equal(CheckPermissionResponse.Types.Permissionship.HasPermission, response.Permissionship);
    }
}
