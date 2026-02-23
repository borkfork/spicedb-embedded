//! Integration tests: hit the Express-style /drive/:folder/:document_id endpoint with X-User header.

use google_docs_example::{create_app, rel};
use spicedb_embedded::v1::{
    RelationshipUpdate, WriteRelationshipsRequest, relationship_update::Operation,
};

#[tokio::test]
async fn alice_viewer_on_folder_can_view_documents() {
    let (app, spicedb) = create_app(Some(vec![])).unwrap();

    let updates = vec![
        RelationshipUpdate {
            operation: Operation::Touch as i32,
            relationship: Some(rel("document:doc1", "folder", "folder:marketing")),
        },
        RelationshipUpdate {
            operation: Operation::Touch as i32,
            relationship: Some(rel("document:doc2", "folder", "folder:marketing")),
        },
    ];
    spicedb
        .permissions()
        .write_relationships(&WriteRelationshipsRequest {
            updates: updates.clone(),
            optional_preconditions: vec![],
            optional_transaction_metadata: None,
        })
        .unwrap();

    let updates = vec![RelationshipUpdate {
        operation: Operation::Touch as i32,
        relationship: Some(rel("folder:marketing", "viewer", "user:alice")),
    }];
    spicedb
        .permissions()
        .write_relationships(&WriteRelationshipsRequest {
            updates,
            optional_preconditions: vec![],
            optional_transaction_metadata: None,
        })
        .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let res1 = client
        .get(format!("http://{}/drive/marketing/doc1", addr))
        .header("X-User", "alice")
        .send()
        .await
        .unwrap();
    let res2 = client
        .get(format!("http://{}/drive/marketing/doc2", addr))
        .header("X-User", "alice")
        .send()
        .await
        .unwrap();

    assert_eq!(res1.status(), 200);
    let body: serde_json::Value = res1.json().await.unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["user"], "alice");
    assert_eq!(body["document"], "doc1");

    assert_eq!(res2.status(), 200);
    let body: serde_json::Value = res2.json().await.unwrap();
    assert_eq!(body["document"], "doc2");
}

#[tokio::test]
async fn bob_denied_when_not_viewer() {
    let (app, _) = create_app(Some(vec![
        rel("document:doc1", "folder", "folder:marketing"),
        rel("folder:marketing", "viewer", "user:alice"),
    ]))
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/drive/marketing/doc1", addr))
        .header("X-User", "bob")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"], "Forbidden");
}

#[tokio::test]
async fn inherits_view_from_parent_folder() {
    let (app, _spicedb) = create_app(Some(vec![
        rel("folder:marketing", "parent", "folder:root"),
        rel("document:doc1", "folder", "folder:marketing"),
        rel("folder:root", "viewer", "user:alice"),
    ]))
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/drive/marketing/doc1", addr))
        .header("X-User", "alice")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["ok"], true);
}

#[tokio::test]
async fn returns_401_when_x_user_missing() {
    let (app, _spicedb) = create_app(Some(vec![
        rel("document:doc1", "folder", "folder:marketing"),
        rel("folder:marketing", "viewer", "user:alice"),
    ]))
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/drive/marketing/doc1", addr))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"], "Missing X-User header");
}
