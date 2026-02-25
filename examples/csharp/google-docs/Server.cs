using Authzed.Api.V1;
using Borkfork.SpiceDb.Embedded;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Http;

namespace Borkfork.SpiceDb.Examples.GoogleDocs;

/// <summary>Google Docs–style server: GET /drive/{folder}/{documentId} with X-User header.</summary>
public static class Server
{
    public static Relationship Rel(string resource, string relation, string subject)
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

    private static List<Relationship> SeedRelationships() =>
    [
        Rel("document:doc1", "folder", "folder:marketing"),
        Rel("document:doc2", "folder", "folder:marketing"),
        Rel("document:spec", "folder", "folder:engineering"),
        Rel("folder:engineering", "parent", "folder:root"),
        Rel("folder:marketing", "parent", "folder:root")
    ];

    /// <summary>Create app and SpiceDB. Pass null for default seed, or a list for custom (e.g. empty).</summary>
    public static (WebApplication App, EmbeddedSpiceDb Spicedb) CreateApp(IReadOnlyList<Relationship>? initialRelationships = null)
    {
        var relationships = initialRelationships ?? SeedRelationships();
        var spicedb = EmbeddedSpiceDb.Start(Schema.DriveSchema, relationships);

        var builder = WebApplication.CreateBuilder();
        var app = builder.Build();

        app.MapGet("/drive/{folder}/{documentId}", (string folder, string documentId, HttpRequest request) =>
        {
            var user = request.Headers["X-User"].FirstOrDefault()?.Trim();
            if (string.IsNullOrEmpty(user))
                return Results.Json(new { error = "Missing X-User header" }, statusCode: 401);

            var checkReq = new CheckPermissionRequest
            {
                Consistency = new Consistency { FullyConsistent = true },
                Resource = new ObjectReference { ObjectType = "document", ObjectId = documentId },
                Permission = "view",
                Subject = new SubjectReference { Object = new ObjectReference { ObjectType = "user", ObjectId = user } }
            };

            try
            {
                var response = spicedb.Permissions().CheckPermission(checkReq);
                if (response.Permissionship != CheckPermissionResponse.Types.Permissionship.HasPermission)
                    return Results.Json(new
                    {
                        error = "Forbidden",
                        message = $"User {user} does not have view access to document {documentId} in folder {folder}"
                    }, statusCode: 403);

                return Results.Json(new { ok = true, user, folder, document = documentId, message = "Access allowed" });
            }
            catch (Exception e)
            {
                return Results.Json(new { error = "Internal error", message = e.Message }, statusCode: 500);
            }
        });

        return (app, spicedb);
    }
}
