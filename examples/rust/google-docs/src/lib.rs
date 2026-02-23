//! Library for the Google Docs example: create app and helpers for tests.

mod schema;

pub use schema::DRIVE_SCHEMA;

use axum::{Json, Router, extract::Path, http::StatusCode, response::IntoResponse, routing::get};
use std::sync::Arc;

/// Build initial relationships for a small demo hierarchy.
pub fn seed_relationships() -> Vec<spicedb_embedded::v1::Relationship> {
    vec![
        rel("document:doc1", "folder", "folder:marketing"),
        rel("document:doc2", "folder", "folder:marketing"),
        rel("document:spec", "folder", "folder:engineering"),
        rel("folder:engineering", "parent", "folder:root"),
        rel("folder:marketing", "parent", "folder:root"),
    ]
}

/// Create a relationship from "resource_type:id", relation, "subject_type:id".
pub fn rel(resource: &str, relation: &str, subject: &str) -> spicedb_embedded::v1::Relationship {
    let (res_type, res_id) = resource.split_once(':').unwrap();
    let (sub_type, sub_id) = subject.split_once(':').unwrap();
    spicedb_embedded::v1::Relationship {
        resource: Some(spicedb_embedded::v1::ObjectReference {
            object_type: res_type.into(),
            object_id: res_id.into(),
        }),
        relation: relation.into(),
        subject: Some(spicedb_embedded::v1::SubjectReference {
            object: Some(spicedb_embedded::v1::ObjectReference {
                object_type: sub_type.into(),
                object_id: sub_id.into(),
            }),
            optional_relation: String::new(),
        }),
        optional_caveat: None,
        optional_expires_at: None,
    }
}

/// Create the app and SpiceDB instance. Pass `None` for default seed, or `Some(rels)` for custom (e.g. empty).
/// Returns `Router<()>` (state is applied via `with_state`), which works with `axum::serve`.
pub fn create_app(
    initial_relationships: Option<Vec<spicedb_embedded::v1::Relationship>>,
) -> Result<(Router<()>, Arc<spicedb_embedded::EmbeddedSpiceDB>), spicedb_embedded::SpiceDBError> {
    let relationships = initial_relationships.unwrap_or_else(seed_relationships);
    let spicedb = Arc::new(spicedb_embedded::EmbeddedSpiceDB::new(
        DRIVE_SCHEMA,
        &relationships,
        None,
    )?);

    let app = Router::new()
        .route("/drive/{folder}/{document_id}", get(drive_handler))
        .with_state(spicedb.clone());

    Ok((app, spicedb))
}

pub async fn drive_handler(
    axum::extract::State(spicedb): axum::extract::State<Arc<spicedb_embedded::EmbeddedSpiceDB>>,
    Path((folder_id, document_id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let user = headers
        .get("x-user")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let user = match user {
        Some(u) => u.to_string(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Missing X-User header" })),
            )
                .into_response();
        }
    };

    let consistency = spicedb_embedded::v1::Consistency {
        requirement: Some(spicedb_embedded::v1::consistency::Requirement::FullyConsistent(true)),
    };
    let request = spicedb_embedded::v1::CheckPermissionRequest {
        consistency: Some(consistency),
        resource: Some(spicedb_embedded::v1::ObjectReference {
            object_type: "document".into(),
            object_id: document_id.clone(),
        }),
        permission: "view".into(),
        subject: Some(spicedb_embedded::v1::SubjectReference {
            object: Some(spicedb_embedded::v1::ObjectReference {
                object_type: "user".into(),
                object_id: user.clone(),
            }),
            optional_relation: String::new(),
        }),
        context: None,
        with_tracing: false,
    };

    match spicedb.permissions().check_permission(&request) {
        Ok(response) => {
            if response.permissionship
                == spicedb_embedded::v1::check_permission_response::Permissionship::HasPermission
                    as i32
            {
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "ok": true,
                        "user": user,
                        "folder": folder_id,
                        "document": document_id,
                        "message": "Access allowed"
                    })),
                )
                    .into_response()
            } else {
                (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "Forbidden",
                        "message": format!("User {} does not have view access to document {} in folder {}", user, document_id, folder_id)
                    })),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Internal error",
                "message": e.to_string()
            })),
        )
            .into_response(),
    }
}
