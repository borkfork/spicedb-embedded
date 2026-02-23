//! Google Docs–style example: GET /drive/:folder/:document_id
//! Uses X-User header as the requesting user and checks SpiceDB for view permission.

use google_docs_example::create_app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let (app, _spicedb) = create_app(None).map_err(|e| e.to_string())?;
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
        eprintln!("Server listening on http://localhost:3000");
        eprintln!("Try: curl -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1");
        axum::serve(listener, app).await?;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .map_err(|e: Box<dyn std::error::Error + Send + Sync>| -> Box<dyn std::error::Error> { e })?;
    Ok(())
}
