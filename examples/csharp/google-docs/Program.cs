using Borkfork.SpiceDb.Examples.GoogleDocs;

var (app, _) = Server.CreateApp();
app.Urls.Add("http://0.0.0.0:3000");
Console.Error.WriteLine("Server listening on http://localhost:3000");
Console.Error.WriteLine("Try: curl -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1");
await app.RunAsync();
