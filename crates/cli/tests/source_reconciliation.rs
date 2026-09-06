//! Process-level DAN-15: consume furnace library-canonical roles.
//! No API. Does not invent product verbs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn skl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_skl"))
}

fn run_skl(home: &Path, cwd: Option<&Path>, args: &[&str]) -> std::process::Output {
    let data = home.join(".local/share/skl");
    let config = home.join(".config/skl");
    let mut cmd = skl();
    cmd.args(args)
        .env("HOME", home)
        .env("SKL_DATA_DIR", &data)
        .env("SKL_CONFIG_DIR", &config)
        .env_remove("SKL_TOKEN")
        .env_remove("SKL_TOKEN_FILE")
        .env("SKL_NO_PROMPT", "1")
        .env("API_BASE", "http://127.0.0.1:1")
        .stdin(Stdio::null());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.output().expect("run skl")
}

fn write_no_auto(home: &Path) {
    let cfg = home.join(".config/skl");
    fs::create_dir_all(&cfg).unwrap();
    fs::write(
        cfg.join("config.toml"),
        "[sync]\nauto = false\nfrequency_secs = 900\n",
    )
    .unwrap();
}

fn plant(dir: &Path, body: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("SKILL.md"), body).unwrap();
}

fn library_of(home: &Path, name: &str) -> PathBuf {
    home.join(".local/share/skl/skills").join(name)
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn assert_success(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "status={} stdout={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn assert_no_merge_ui(text: &str) {
    for needle in [
        "keep [l]ocal",
        "overwrite? ",
        "[y/n]",
        "three-way",
        "merge conflict",
        "agent-dir merge",
    ] {
        assert!(!text.contains(needle), "merge UI {needle:?} in {text}");
    }
}

#[test]
fn init_puts_skill_in_library_only() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_no_auto(&home);
    plant(
        &home.join(".claude/skills/greeter"),
        "# greeter\nhello from claude\n",
    );

    let out = run_skl(&home, None, &["init"]);
    assert_success(&out);
    let text = combined(&out);
    assert!(text.contains("Imported 1 skill"), "{text}");
    assert!(text.contains("personal library only"), "{text}");
    assert_no_merge_ui(&text);

    let lib = library_of(&home, "greeter");
    assert_eq!(
        fs::read_to_string(lib.join("SKILL.md")).unwrap(),
        "# greeter\nhello from claude\n"
    );
    assert_eq!(
        fs::read_to_string(home.join(".claude/skills/greeter/SKILL.md")).unwrap(),
        "# greeter\nhello from claude\n"
    );
    assert!(!home.join(".agents/skills/greeter").exists());
    assert!(!home.join(".config/agents/skills/greeter").exists());
}

#[test]
fn use_link_points_at_library() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).unwrap();
    write_no_auto(&home);
    plant(&home.join(".claude/skills/greeter"), "# greeter\nlib\n");
    assert_success(&run_skl(&home, None, &["init"]));

    let out = run_skl(
        &home,
        None,
        &["use", "greeter", "--project", project.to_str().unwrap()],
    );
    assert_success(&out);
    let text = combined(&out);
    assert!(text.contains("using greeter"), "{text}");
    assert_no_merge_ui(&text);

    let dest = project.join(".agents/skills/greeter");
    assert!(dest.is_symlink(), "use must project a symlink");
    let target = fs::read_link(&dest).unwrap();
    let target_real = if target.is_absolute() {
        fs::canonicalize(&target).unwrap()
    } else {
        fs::canonicalize(dest.parent().unwrap().join(&target)).unwrap()
    };
    let lib_real = fs::canonicalize(library_of(&home, "greeter")).unwrap();
    assert_eq!(target_real, lib_real);
    assert!(!project.join(".claude").exists());
}

#[test]
fn doctor_warns_on_divergent_copy_then_capture_and_use_reproject() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).unwrap();
    write_no_auto(&home);
    plant(&home.join(".claude/skills/greeter"), "# greeter\nlibrary\n");
    assert_success(&run_skl(&home, None, &["init"]));
    assert_success(&run_skl(
        &home,
        None,
        &["use", "greeter", "--project", project.to_str().unwrap()],
    ));

    let dest = project.join(".agents/skills/greeter");
    fs::remove_file(&dest).unwrap_or_else(|_| {
        fs::remove_dir_all(&dest).unwrap();
    });
    plant(&dest, "# greeter\ndivergent project copy\n");
    assert!(!dest.is_symlink());

    let before = fs::read_to_string(dest.join("SKILL.md")).unwrap();
    let doctor = run_skl(&home, Some(&project), &["doctor"]);
    assert_success(&doctor);
    let doc = combined(&doctor);
    assert!(doc.contains("not a sync peer"), "{doc}");
    assert!(doc.contains("skl capture"), "{doc}");
    assert!(doc.contains("personal library is canonical"), "{doc}");
    assert_no_merge_ui(&doc);
    assert_eq!(
        fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        before,
        "doctor must not mutate the divergent copy"
    );
    assert!(!dest.is_symlink());

    let cap = run_skl(
        &home,
        None,
        &[
            "capture",
            ".agents/skills/greeter",
            "--force",
            "--project",
            project.to_str().unwrap(),
        ],
    );
    assert_success(&cap);
    let cap_text = combined(&cap);
    assert!(cap_text.contains("captured greeter"), "{cap_text}");
    assert_no_merge_ui(&cap_text);
    assert_eq!(
        fs::read_to_string(library_of(&home, "greeter").join("SKILL.md")).unwrap(),
        "# greeter\ndivergent project copy\n"
    );

    let used = run_skl(
        &home,
        None,
        &["use", "greeter", "--project", project.to_str().unwrap()],
    );
    assert_success(&used);
    assert_no_merge_ui(&combined(&used));
    assert!(dest.is_symlink(), "use must re-project a library link");
    let target = fs::read_link(&dest).unwrap();
    let target_real = if target.is_absolute() {
        fs::canonicalize(&target).unwrap()
    } else {
        fs::canonicalize(dest.parent().unwrap().join(&target)).unwrap()
    };
    let lib_real = fs::canonicalize(library_of(&home, "greeter")).unwrap();
    assert_eq!(target_real, lib_real);
    assert_eq!(
        fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "# greeter\ndivergent project copy\n"
    );

    let after = run_skl(&home, Some(&project), &["doctor"]);
    assert_success(&after);
    assert!(
        !combined(&after).contains("not a sync peer"),
        "{}",
        combined(&after)
    );
}
