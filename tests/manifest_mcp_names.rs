#![cfg(test)]
use arnes::manifest;

#[test]
fn accepts_native_server_identifiers_without_renaming_them()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for name in ["node_repl", "Context7", "context7-http"] {
        let input = format!(
            "version: 1\nagents:\n  - id: codex\n    scopes: [user]\nmcp:\n  - {{ name: {name}, agent: codex, scope: user, command: mcp }}\nresources: []\n"
        );
        let manifest = manifest::parse(&input)?;
        assert_eq!(
            manifest
                .mcp_registrations()
                .next()
                .ok_or("missing server")?
                .name,
            name
        );
    }
    Ok(())
}
