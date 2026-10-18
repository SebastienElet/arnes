use std::{fs, process::Command};

#[test]
fn export_selects_explicit_sources_and_refuses_unsafe_or_missing_files()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(root)
            .status()?
            .success()
    );
    fs::write(root.join("instructions.md"), "public rule\n")?;
    let selection = root.join("export.json");
    fs::write(&selection, r#"{"instructions.md":"instructions"}"#)?;
    let invoke = |check: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_arnes"));
        command
            .args(["export", "--sources"])
            .arg(&selection)
            .current_dir(root)
            .env("HOME", root);
        if check {
            command.arg("--check");
        }
        command.output()
    };
    let output = invoke(false)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(invoke(true)?.status.success());
    for contents in [
        r#"{"../outside":"instructions"}"#,
        r#"{"credentials.json":"services"}"#,
        r#"{"missing.md":"instructions"}"#,
        "{}",
        "null",
    ] {
        fs::write(&selection, contents)?;
        assert!(!invoke(false)?.status.success(), "{contents}");
    }
    fs::write(&selection, r#"{"instructions.md":"instructions"}"#)?;
    fs::write(root.join("instructions.md"), "changed\n")?;
    assert!(!invoke(true)?.status.success());
    Ok(())
}
