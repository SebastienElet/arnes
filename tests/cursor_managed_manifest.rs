#![cfg(test)]
mod support;

use std::fs;
use std::os::unix::fs::symlink;
use support::Fixture;

const DESTINATION: &str = ".cursor/plugins/local/dotfiles-harness/.cursor-plugin/plugin.json";
const SOURCE: &str = "home/cursor-plugin.json";

fn projected_fixture() -> Result<Fixture, Box<dyn std::error::Error + Send + Sync>> {
    let fixture = Fixture::new()?;
    let declaration = format!(
        "version: 1\nagents:\n  - id: cursor\n    scopes: [user]\nexternal:\n  plugins:\n    - {{ agent: cursor, scope: user, id: dotfiles-harness }}\nresources:\n  - id: cursor-manifest\n    kind: config\n    agent: cursor\n    scope: user\n    source: {{ root: repository, path: {SOURCE} }}\n    destination: {{ root: home, path: {DESTINATION} }}"
    );
    fixture.write_home(".arnes.yaml", &declaration)?;
    fixture.write_repository(SOURCE, "{\"name\":\"dotfiles-harness\"}\n")?;
    let destination = fixture.home().join(DESTINATION);
    fs::create_dir_all(destination.parent().ok_or("missing parent")?)?;
    symlink(fixture.repository().join(SOURCE), destination)?;
    Ok(fixture)
}

fn inspect(
    fixture: &Fixture,
) -> Result<(i32, String, String), Box<dyn std::error::Error + Send + Sync>> {
    let before = fixture.snapshot()?;
    let output = fixture.command(["doctor", "skills", "--agent", "cursor", "--scope", "user"])?;
    assert_eq!(fixture.snapshot()?, before);
    Ok((
        output.status.code().ok_or("missing status")?,
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

#[test]
fn accepts_the_exact_declared_cursor_manifest_projection_without_writes()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = projected_fixture()?;
    let before = fixture.snapshot()?;
    let (code, stdout, _) = inspect(&fixture)?;
    assert_eq!(code, 0, "{stdout}");
    assert!(!stdout.contains("broken"), "{stdout}");
    assert_eq!(fixture.snapshot()?, before);
    Ok(())
}

#[test]
fn rejects_a_different_repository_file_despite_a_declared_projection()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = projected_fixture()?;
    fixture.write_repository("home/other.json", "{\"name\":\"dotfiles-harness\"}\n")?;
    fs::remove_file(fixture.home().join(DESTINATION))?;
    symlink(
        fixture.repository().join("home/other.json"),
        fixture.home().join(DESTINATION),
    )?;
    let (code, stdout, _) = inspect(&fixture)?;
    assert_eq!(code, 2, "{stdout}");
    assert!(
        stdout.contains("plugin manifest resolves outside its root"),
        "{stdout}"
    );
    Ok(())
}

#[test]
fn rejects_a_declared_source_link_escaping_the_repository()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = projected_fixture()?;
    fixture.write_home("foreign.json", "{\"name\":\"dotfiles-harness\"}\n")?;
    fs::remove_file(fixture.repository().join(SOURCE))?;
    symlink(
        fixture.home().join("foreign.json"),
        fixture.repository().join(SOURCE),
    )?;
    let (code, stdout, _) = inspect(&fixture)?;
    assert_eq!(code, 2, "{stdout}");
    assert!(
        stdout.contains("plugin manifest resolves outside its root"),
        "{stdout}"
    );
    Ok(())
}

#[test]
fn rejects_projection_routes_detouring_through_an_undeclared_root()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for directory_link in [true, false] {
        let fixture = projected_fixture()?;
        let outside = tempfile::tempdir()?;
        let destination = fixture.home().join(DESTINATION);
        fs::remove_file(&destination)?;
        symlink(
            fixture.repository().join(SOURCE),
            outside.path().join("plugin.json"),
        )?;
        if directory_link {
            let parent = destination.parent().ok_or("missing parent")?;
            fs::remove_dir(parent)?;
            symlink(outside.path(), parent)?;
        } else {
            symlink(outside.path().join("plugin.json"), &destination)?;
        }
        let (code, stdout, _) = inspect(&fixture)?;
        assert_eq!(code, 2, "{stdout}");
        assert!(
            stdout.contains("plugin manifest resolves outside its root"),
            "{stdout}"
        );
    }
    Ok(())
}

#[test]
fn rejects_a_sibling_file_relay_even_when_it_resolves_to_the_declared_source()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for repository_relay in [false, true] {
        let fixture = projected_fixture()?;
        let outside = fixture
            .home()
            .parent()
            .ok_or("missing parent")?
            .join("outside.json");
        symlink(fixture.repository().join(SOURCE), &outside)?;
        let target = if repository_relay {
            let relay = fixture.repository().join("relay.json");
            symlink(&outside, &relay)?;
            relay
        } else {
            outside
        };
        fs::remove_file(fixture.home().join(DESTINATION))?;
        symlink(target, fixture.home().join(DESTINATION))?;
        let (code, stdout, _) = inspect(&fixture)?;
        assert_eq!(code, 2, "{stdout}");
    }
    Ok(())
}

#[test]
fn accepts_a_direct_relative_projection() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = projected_fixture()?;
    fs::remove_file(fixture.home().join(DESTINATION))?;
    symlink(
        "../../../../../../repository/home/cursor-plugin.json",
        fixture.home().join(DESTINATION),
    )?;
    let (code, stdout, _) = inspect(&fixture)?;
    assert_eq!(code, 0, "{stdout}");
    Ok(())
}

#[test]
fn rejects_parent_traversal_after_an_external_directory_link()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = projected_fixture()?;
    let outside = tempfile::tempdir()?;
    fs::create_dir(outside.path().join("sub"))?;
    symlink(
        fixture.repository().join(SOURCE),
        outside.path().join("cursor-plugin.json"),
    )?;
    symlink(
        outside.path().join("sub"),
        fixture.repository().join("home/detour"),
    )?;
    fs::remove_file(fixture.home().join(DESTINATION))?;
    symlink(
        fixture
            .repository()
            .join("home/detour/../cursor-plugin.json"),
        fixture.home().join(DESTINATION),
    )?;
    let (code, stdout, _) = inspect(&fixture)?;
    assert_eq!(code, 2, "{stdout}");
    Ok(())
}

#[test]
fn still_validates_projected_json_and_detects_a_missing_source()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fixture = projected_fixture()?;
    fixture.write_repository(SOURCE, "not JSON\n")?;
    let (code, stdout, _) = inspect(&fixture)?;
    assert_eq!(code, 2, "{stdout}");
    assert!(
        stdout.contains("plugin manifest is invalid JSON"),
        "{stdout}"
    );
    fs::remove_file(fixture.repository().join(SOURCE))?;
    let (code, stdout, _) = inspect(&fixture)?;
    assert_eq!(code, 2, "{stdout}");
    assert!(
        stdout.contains("plugin manifest link is dangling"),
        "{stdout}"
    );
    Ok(())
}
