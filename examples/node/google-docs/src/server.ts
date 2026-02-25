/**
 * Google Docs–style example: /drive/:folder/:documentId
 * Uses X-User header as the requesting user and checks SpiceDB for view permission.
 */

import { fileURLToPath } from "node:url";
import express from "express";
import { v1, EmbeddedSpiceDB } from "spicedb-embedded";
import { DRIVE_SCHEMA } from "./schema.js";

const {
  CheckPermissionRequest,
  CheckPermissionResponse_Permissionship,
  Consistency,
  ObjectReference,
  Relationship,
  SubjectReference,
} = v1;

function rel(resource: string, relation: string, subject: string) {
  const [resType, resId] = resource.split(":");
  const [subType, subId] = subject.split(":");
  return Relationship.create({
    resource: ObjectReference.create({ objectType: resType, objectId: resId }),
    relation,
    subject: SubjectReference.create({
      object: ObjectReference.create({ objectType: subType, objectId: subId }),
    }),
  });
}

/** Build initial relationships for a small demo hierarchy. */
function seedRelationships() {
  return [
    rel("document:doc1", "folder", "folder:marketing"),
    rel("document:doc2", "folder", "folder:marketing"),
    rel("document:spec", "folder", "folder:engineering"),
    rel("folder:engineering", "parent", "folder:root"),
    rel("folder:marketing", "parent", "folder:root"),
  ];
}

const HAS_PERMISSION = CheckPermissionResponse_Permissionship.HAS_PERMISSION;

export async function createApp(
  initialRelationships?: v1.Relationship[],
): Promise<{
  app: express.Express;
  spicedb: EmbeddedSpiceDB;
}> {
  const relationships = initialRelationships ?? seedRelationships();
  const spicedb = await EmbeddedSpiceDB.start(DRIVE_SCHEMA, relationships);

  const app = express();

  app.get("/drive/:folder/:documentId", async (req, res) => {
    const folderId = req.params.folder as string;
    const documentId = req.params.documentId as string;
    const user = (req.headers["x-user"] as string)?.trim();

    if (!user) {
      res.status(401).json({ error: "Missing X-User header" });
      return;
    }

    const consistency = Consistency.create({
      requirement: { oneofKind: "fullyConsistent", fullyConsistent: true },
    });
    const checkReq = CheckPermissionRequest.create({
      consistency,
      resource: ObjectReference.create({
        objectType: "document",
        objectId: documentId,
      }),
      permission: "view",
      subject: SubjectReference.create({
        object: ObjectReference.create({
          objectType: "user",
          objectId: user,
        }),
      }),
    });

    try {
      const response = await spicedb
        .permissions()
        .promises.checkPermission(checkReq);

      if (response.permissionship !== HAS_PERMISSION) {
        res.status(403).json({
          error: "Forbidden",
          message: `User ${user} does not have view access to document ${documentId} in folder ${folderId}`,
        });
        return;
      }

      res.json({
        ok: true,
        user,
        folder: folderId,
        document: documentId,
        message: "Access allowed",
      });
    } catch (err) {
      res.status(500).json({
        error: "Internal error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  });

  return { app, spicedb };
}

export async function main(): Promise<void> {
  const { app, spicedb } = await createApp();
  const port = Number(process.env.PORT) || 3000;

  const server = app.listen(port, () => {
    console.log(`Server listening on http://localhost:${port}`);
    console.log(
      "Try: curl -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1",
    );
  });

  const shutdown = (): void => {
    server.close();
    spicedb.close();
    process.exit(0);
  };
  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

// Only start the server when this file is run directly, not when imported (e.g. by tests)
const __filename = fileURLToPath(import.meta.url);
if (process.argv[1] === __filename) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
