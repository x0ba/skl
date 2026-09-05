//! TUI state + key handling. Catalog load is local-only (no network).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::commands::use_cmd::{resolve_activation_extras, resolve_project, resolve_skill};
use crate::config::{self, Paths};
use crate::error::{Result, SklError};
use crate::hooks::conflict::ConflictMode;
use crate::local::db::LocalDb;
use crate::local::linker::{self, LinkAction};
use crate::sync::SyncOptions;

use super::render;
use super::terminal::TuiTerminal;

const HELP_TEXT: &str = "\
skl — local skill browser

↑/↓ or j/k     move in the list (list owns arrows)
[ / ]          scroll preview
Ctrl-j / Ctrl-k  scroll preview
/              search (filter names)
e              edit SKILL.md ($VISUAL or $EDITOR)
u              use in this project (same as `skl use <name>`)
U              unuse in this project (same as `skl unuse <name>`)
s              sync (blocking; same as `skl sync`)
r              refresh from local library / state.db
?              this help
q or Esc       quit (Esc also leaves search / help)

Browse works offline. Sync needs login + API.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRow {
    pub name: String,
    pub path: PathBuf,
    pub activated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub skills: Vec<SkillRow>,
    pub last_sync_at: Option<i64>,
    pub project: PathBuf,
    pub project_label: String,
    pub empty_hint: Option<String>,
    pub load_error: Option<String>,
}

#[derive(Debug)]
pub struct App {
    pub catalog: Catalog,
    pub selected: usize,
    pub preview_scroll: u16,
    pub overlay: Overlay,
    pub query: String,
    pub status: String,
    pub preview: Preview,
    /// After `u`/`U`, piggyback the same fail-soft auto-sync as the CLI verbs.
    pending_auto_sync: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub title: String,
    pub body: String,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Continue,
    Quit,
    /// Leave TUI, run an external action, then reload.
    SuspendSync,
    SuspendEdit,
}

pub async fn run(api_base: String) -> Result<()> {
    let mut term = TuiTerminal::enter()?;
    let mut app = App::load()?;

    loop {
        term.terminal()
            .draw(|frame| render::draw(frame, &app))
            .map_err(|err| SklError::LocalState(format!("TUI draw: {err}")))?;

        let ev = match event::read() {
            Ok(ev) => ev,
            Err(err) => {
                app.status = format!("input error: {err}");
                continue;
            }
        };
        let Event::Key(key) = ev else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.handle_key(key) {
            Tick::Continue => {
                if let Some(verb) = app.pending_auto_sync.take() {
                    if let Ok(paths) = Paths::resolve() {
                        let _ = crate::auto_sync::maybe_run(&api_base, &paths, verb).await;
                    }
                }
            }
            Tick::Quit => break,
            Tick::SuspendSync => {
                term.suspend()?;
                eprintln!("syncing…");
                let result = crate::commands::sync::run(
                    api_base.clone(),
                    SyncOptions {
                        conflict: ConflictMode::Prompt,
                        allow_warnings: false,
                    },
                )
                .await;
                match result {
                    Ok(()) => app.status = "sync finished".into(),
                    Err(err) => app.status = format!("sync: {err}"),
                }
                term.resume()?;
                app.reload();
            }
            Tick::SuspendEdit => {
                let Some(path) = app.selected_skill_md_path() else {
                    app.status = "no skill selected".into();
                    continue;
                };
                term.suspend()?;
                if let Err(err) = open_editor(&path) {
                    eprintln!("edit: {err}");
                    app.status = format!("edit: {err}");
                } else {
                    app.status = format!("edited {}", path.display());
                }
                term.resume()?;
                app.reload();
            }
        }
    }
    Ok(())
}

impl App {
    pub fn load() -> Result<Self> {
        let catalog = load_catalog()?;
        let mut app = Self {
            catalog,
            selected: 0,
            preview_scroll: 0,
            overlay: Overlay::None,
            query: String::new(),
            status: String::new(),
            preview: Preview {
                title: String::new(),
                body: String::new(),
                warning: None,
            },
            pending_auto_sync: None,
        };
        app.refresh_preview();
        Ok(app)
    }

    pub fn reload(&mut self) {
        match load_catalog() {
            Ok(catalog) => {
                self.catalog = catalog;
                self.clamp_selected();
                self.refresh_preview();
            }
            Err(err) => {
                self.status = format!("refresh: {err}");
            }
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        filter_indices(&self.catalog.skills, &self.query)
    }

    pub fn selected_row(&self) -> Option<&SkillRow> {
        let idxs = self.filtered_indices();
        idxs.get(self.selected)
            .and_then(|i| self.catalog.skills.get(*i))
    }

    pub fn selected_skill_md_path(&self) -> Option<PathBuf> {
        self.selected_row().map(|row| row.path.join("SKILL.md"))
    }

    pub fn header_line(&self, now: i64) -> String {
        format_header(&self.catalog, now)
    }

    fn clamp_selected(&mut self) {
        let n = self.filtered_indices().len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    fn move_sel(&mut self, delta: isize) {
        let n = self.filtered_indices().len();
        if n == 0 {
            self.selected = 0;
            self.preview_scroll = 0;
            self.refresh_preview();
            return;
        }
        let next = (self.selected as isize + delta).clamp(0, n as isize - 1) as usize;
        if next != self.selected {
            self.selected = next;
            self.preview_scroll = 0;
            self.refresh_preview();
        }
    }

    fn scroll_preview(&mut self, delta: i16) {
        let next = self.preview_scroll as i32 + i32::from(delta);
        self.preview_scroll = next.clamp(0, i32::from(u16::MAX)) as u16;
    }

    pub fn refresh_preview(&mut self) {
        self.preview = match self.selected_row() {
            Some(row) => load_preview(row),
            None => Preview {
                title: "SKILL.md".into(),
                body: self
                    .catalog
                    .empty_hint
                    .clone()
                    .unwrap_or_else(|| "No skill selected.".into()),
                warning: None,
            },
        };
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Tick {
        if self.overlay == Overlay::Help {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.overlay = Overlay::None;
                }
                _ => {}
            }
            return Tick::Continue;
        }

        if self.overlay == Overlay::Search {
            return self.handle_search_key(key);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => Tick::Quit,
            (KeyCode::Char('?'), _) => {
                self.overlay = Overlay::Help;
                Tick::Continue
            }
            (KeyCode::Char('/'), _) => {
                self.overlay = Overlay::Search;
                Tick::Continue
            }
            (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.move_sel(-1);
                Tick::Continue
            }
            (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.move_sel(1);
                Tick::Continue
            }
            (KeyCode::Char('['), _) => {
                self.scroll_preview(-1);
                Tick::Continue
            }
            (KeyCode::Char(']'), _) => {
                self.scroll_preview(1);
                Tick::Continue
            }
            (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                self.scroll_preview(-1);
                Tick::Continue
            }
            (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                self.scroll_preview(1);
                Tick::Continue
            }
            (KeyCode::Char('r'), _) => {
                self.reload();
                self.status = "refreshed".into();
                Tick::Continue
            }
            (KeyCode::Char('e'), _) => Tick::SuspendEdit,
            (KeyCode::Char('u'), KeyModifiers::NONE) => {
                self.activate_selected();
                Tick::Continue
            }
            (KeyCode::Char('U'), _) => {
                self.deactivate_selected();
                Tick::Continue
            }
            (KeyCode::Char('s'), _) => Tick::SuspendSync,
            _ => Tick::Continue,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> Tick {
        match key.code {
            KeyCode::Esc => {
                self.query.clear();
                self.overlay = Overlay::None;
                self.selected = 0;
                self.preview_scroll = 0;
                self.refresh_preview();
            }
            KeyCode::Enter => {
                self.overlay = Overlay::None;
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
                self.preview_scroll = 0;
                self.refresh_preview();
            }
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Down => self.move_sel(1),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.push(c);
                self.selected = 0;
                self.preview_scroll = 0;
                self.refresh_preview();
            }
            _ => {}
        }
        Tick::Continue
    }

    fn activate_selected(&mut self) {
        let Some(name) = self.selected_row().map(|r| r.name.clone()) else {
            self.status = "no skill selected".into();
            return;
        };
        match activate_cwd(&name) {
            Ok(msg) => {
                self.status = msg;
                self.pending_auto_sync = Some("use");
                self.reload();
            }
            Err(err) => self.status = format!("use: {err}"),
        }
    }

    fn deactivate_selected(&mut self) {
        let Some(name) = self.selected_row().map(|r| r.name.clone()) else {
            self.status = "no skill selected".into();
            return;
        };
        match deactivate_cwd(&name) {
            Ok(msg) => {
                self.status = msg;
                self.pending_auto_sync = Some("unuse");
                self.reload();
            }
            Err(err) => self.status = format!("unuse: {err}"),
        }
    }
}

pub fn help_text() -> &'static str {
    HELP_TEXT
}

/// Same resolution + linker path as `skl use <name>` (cwd only, no `-a`).
pub fn activate_cwd(name: &str) -> Result<String> {
    let project = resolve_project(None)?;
    let home = config::home_dir()?;
    let paths = Paths::resolve().ok();
    let db_file = paths.as_ref().map(|p| p.db_file.as_path());
    let extras = resolve_activation_extras(paths.as_ref(), &[])?;
    let skill = resolve_skill(name, &home, db_file)?;
    let out = linker::activate_with_extras(&project, &home, &skill, &extras)?;
    Ok(format!(
        "using {}  ({}  {})",
        out.skill,
        out.source,
        out.source_path.display()
    ))
}

/// Same linker path as `skl unuse <name>` (cwd only).
pub fn deactivate_cwd(name: &str) -> Result<String> {
    let project = resolve_project(None)?;
    let home = config::home_dir()?;
    let paths = Paths::resolve().ok();
    let extras = resolve_activation_extras(paths.as_ref(), &[])?;
    let out = linker::deactivate_with_extras(&project, &home, name, &extras)?;
    let removed = out
        .links
        .iter()
        .any(|link| link.action == LinkAction::Removed);
    let _ = removed;
    Ok(format!("unused {}", out.skill))
}

pub fn filter_indices(skills: &[SkillRow], query: &str) -> Vec<usize> {
    let q = query.trim().to_ascii_lowercase();
    skills
        .iter()
        .enumerate()
        .filter(|(_, row)| q.is_empty() || row.name.to_ascii_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

pub fn format_header(catalog: &Catalog, now: i64) -> String {
    let n = catalog.skills.len();
    let age = format_sync_age(catalog.last_sync_at, now);
    format!(
        "skl  {n} skill{}  last sync {age}  {}",
        if n == 1 { "" } else { "s" },
        catalog.project_label
    )
}

pub fn format_sync_age(last_sync_at: Option<i64>, now: i64) -> String {
    let Some(at) = last_sync_at else {
        return "never".into();
    };
    if at <= 0 {
        return "never".into();
    }
    let secs = now.saturating_sub(at).max(0) as u64;
    if secs < 5 {
        return "just now".into();
    }
    let d = std::time::Duration::from_secs(secs);
    format!("{} ago", humantime::format_duration(d))
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn load_catalog() -> Result<Catalog> {
    let project = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let paths = Paths::resolve().ok();
    load_catalog_at(&project, paths.as_ref(), now_secs())
}

pub fn load_catalog_at(project: &Path, paths: Option<&Paths>, _now: i64) -> Result<Catalog> {
    let activated = activated_names(project);
    let mut skills: Vec<SkillRow> = Vec::new();
    let mut load_error = None;
    let mut last_sync_at = None;

    if let Some(paths) = paths {
        match scan_library(&paths.library_dir()) {
            Ok(found) => {
                for path in found {
                    let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string)
                    else {
                        continue;
                    };
                    let activated = activated.contains(&name);
                    push_unique(
                        &mut skills,
                        SkillRow {
                            name,
                            path,
                            activated,
                        },
                    );
                }
            }
            Err(err) => load_error = Some(err.to_string()),
        }

        if paths.db_file.exists() {
            match LocalDb::open(&paths.db_file) {
                Ok(db) => {
                    last_sync_at = db.last_sync_summary().ok().flatten().map(|(at, _)| at);
                    if let Ok(indexed) = db.list_skills() {
                        for skill in indexed {
                            push_unique(
                                &mut skills,
                                SkillRow {
                                    name: skill.name.clone(),
                                    activated: activated.contains(&skill.name),
                                    path: skill.path,
                                },
                            );
                        }
                    }
                }
                Err(err) => {
                    load_error = Some(format!("state.db: {err}"));
                }
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    // Re-apply activated after merge (library row may precede db row).
    for row in &mut skills {
        row.activated = activated.contains(&row.name);
    }

    let empty_hint = if skills.is_empty() {
        Some(empty_catalog_hint(paths))
    } else {
        None
    };

    Ok(Catalog {
        skills,
        last_sync_at,
        project: project.to_path_buf(),
        project_label: project_label(project),
        empty_hint,
        load_error,
    })
}

fn empty_catalog_hint(paths: Option<&Paths>) -> String {
    let lib = paths
        .map(|p| p.library_dir())
        .unwrap_or_else(|| PathBuf::from("~/.local/share/skl/skills"));
    format!(
        "No local skills.\nRun `skl init` then `skl sync` to fill {}.\nBrowsing offline is fine — this screen never talks to the network.",
        lib.display()
    )
}

fn project_label(project: &Path) -> String {
    project
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| project.display().to_string())
}

fn activated_names(project: &Path) -> std::collections::BTreeSet<String> {
    linker::load_manifest(project)
        .map(|m| m.skills.into_iter().map(|s| s.name).collect())
        .unwrap_or_default()
}

fn scan_library(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    if !dir.is_dir() {
        return Err(SklError::LocalState(format!(
            "library is not a directory: {}",
            dir.display()
        )));
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if !name.starts_with('.') {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

fn push_unique(skills: &mut Vec<SkillRow>, row: SkillRow) {
    if skills.iter().any(|s| s.name == row.name) {
        return;
    }
    skills.push(row);
}

pub fn load_preview(row: &SkillRow) -> Preview {
    let md = row.path.join("SKILL.md");
    if !md.exists() {
        return Preview {
            title: format!("{} / SKILL.md", row.name),
            body: String::new(),
            warning: Some(format!(
                "SKILL.md missing at {} — skill may be incomplete; try `skl sync`.",
                md.display()
            )),
        };
    }
    match fs::read_to_string(&md) {
        Ok(body) => Preview {
            title: format!("{} / SKILL.md", row.name),
            body,
            warning: None,
        },
        Err(err) => Preview {
            title: format!("{} / SKILL.md", row.name),
            body: String::new(),
            warning: Some(format!("cannot read SKILL.md: {err}")),
        },
    }
}

fn open_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into());
    let mut parts = editor.split_whitespace();
    let bin = parts.next().unwrap_or("vi");
    let mut cmd = std::process::Command::new(bin);
    for arg in parts {
        cmd.arg(arg);
    }
    cmd.arg(path);
    let status = cmd
        .status()
        .map_err(|err| SklError::LocalState(format!("spawn {bin}: {err}")))?;
    if !status.success() {
        return Err(SklError::LocalState(format!(
            "{bin} exited {}",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::skills::{hash_skill_dir, DiscoveredSkill};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn sample_catalog() -> Catalog {
        Catalog {
            skills: vec![
                SkillRow {
                    name: "alpha".into(),
                    path: PathBuf::from("/tmp/alpha"),
                    activated: true,
                },
                SkillRow {
                    name: "beta".into(),
                    path: PathBuf::from("/tmp/beta"),
                    activated: false,
                },
                SkillRow {
                    name: "gamma".into(),
                    path: PathBuf::from("/tmp/gamma"),
                    activated: false,
                },
            ],
            last_sync_at: Some(1_700_000_000),
            project: PathBuf::from("/tmp/proj"),
            project_label: "proj".into(),
            empty_hint: None,
            load_error: None,
        }
    }

    fn app_with(catalog: Catalog) -> App {
        let mut app = App {
            catalog,
            selected: 0,
            preview_scroll: 0,
            overlay: Overlay::None,
            query: String::new(),
            status: String::new(),
            preview: Preview {
                title: String::new(),
                body: "body".into(),
                warning: None,
            },
            pending_auto_sync: None,
        };
        app.refresh_preview();
        app
    }

    #[test]
    fn arrows_and_jk_move_list_not_preview() {
        let mut app = app_with(sample_catalog());
        assert_eq!(app.selected, 0);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 1);
        assert_eq!(app.preview_scroll, 0);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.selected, 2);
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.selected, 1);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn preview_scroll_keys_do_not_move_list() {
        let mut app = app_with(sample_catalog());
        app.handle_key(key(KeyCode::Char(']')));
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.selected, 0);
        assert_eq!(app.preview_scroll, 2);
        app.handle_key(key(KeyCode::Char('[')));
        assert_eq!(app.preview_scroll, 1);
        app.handle_key(ctrl('j'));
        assert_eq!(app.preview_scroll, 2);
        app.handle_key(ctrl('k'));
        assert_eq!(app.preview_scroll, 1);
    }

    #[test]
    fn slash_search_filters_and_esc_clears() {
        let mut app = app_with(sample_catalog());
        app.handle_key(key(KeyCode::Char('/')));
        assert_eq!(app.overlay, Overlay::Search);
        app.handle_key(key(KeyCode::Char('g')));
        app.handle_key(key(KeyCode::Char('a')));
        assert_eq!(app.filtered_indices().len(), 1);
        assert_eq!(app.selected_row().unwrap().name, "gamma");
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.query.is_empty());
        assert_eq!(app.filtered_indices().len(), 3);
    }

    #[test]
    fn q_and_esc_quit_from_browse() {
        let mut app = app_with(sample_catalog());
        assert_eq!(app.handle_key(key(KeyCode::Char('q'))), Tick::Quit);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Tick::Quit);
    }

    #[test]
    fn help_toggle_does_not_quit() {
        let mut app = app_with(sample_catalog());
        app.handle_key(key(KeyCode::Char('?')));
        assert_eq!(app.overlay, Overlay::Help);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Tick::Continue);
        assert_eq!(app.overlay, Overlay::None);
    }

    #[test]
    fn e_and_s_suspend() {
        let mut app = app_with(sample_catalog());
        assert_eq!(app.handle_key(key(KeyCode::Char('e'))), Tick::SuspendEdit);
        assert_eq!(app.handle_key(key(KeyCode::Char('s'))), Tick::SuspendSync);
    }

    #[test]
    fn header_shows_count_age_and_project() {
        let cat = sample_catalog();
        let line = format_header(&cat, 1_700_003_600);
        assert!(line.contains("3 skills"), "{line}");
        assert!(line.contains("last sync"), "{line}");
        assert!(line.contains("proj"), "{line}");
        assert!(line.contains("ago") || line.contains("just now"), "{line}");
    }

    #[test]
    fn sync_age_never_when_missing() {
        assert_eq!(format_sync_age(None, 100), "never");
        assert_eq!(format_sync_age(Some(0), 100), "never");
        assert_eq!(format_sync_age(Some(99), 100), "just now");
    }

    #[test]
    fn catalog_from_library_and_manifest_marks_activated() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let lib = data.join("skills/greeter");
        fs::create_dir_all(&lib).unwrap();
        fs::write(lib.join("SKILL.md"), "# hi\n").unwrap();
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let db = LocalDb::open(&data.join("state.db")).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "agents".into(),
            path: lib.clone(),
            tree: hash_skill_dir(&lib).unwrap(),
        }])
        .unwrap();
        linker::activate(&project, &home, &DiscoveredSkill {
            name: "greeter".into(),
            source: "agents".into(),
            path: lib.clone(),
            tree: hash_skill_dir(&lib).unwrap(),
        })
        .unwrap();

        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: data,
            db_file: tmp.path().join("data/state.db"),
        };
        let cat = load_catalog_at(&project, Some(&paths), 0).unwrap();
        assert_eq!(cat.skills.len(), 1);
        assert_eq!(cat.skills[0].name, "greeter");
        assert!(cat.skills[0].activated);
        assert!(cat.empty_hint.is_none());
    }

    #[test]
    fn empty_catalog_hints_init_sync() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: tmp.path().join("data"),
            db_file: tmp.path().join("data/state.db"),
        };
        let cat = load_catalog_at(&project, Some(&paths), 0).unwrap();
        assert!(cat.skills.is_empty());
        let hint = cat.empty_hint.unwrap();
        assert!(hint.contains("skl init"), "{hint}");
        assert!(hint.contains("skl sync"), "{hint}");
    }

    #[test]
    fn missing_skill_md_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("orphan");
        fs::create_dir_all(&dir).unwrap();
        let preview = load_preview(&SkillRow {
            name: "orphan".into(),
            path: dir,
            activated: false,
        });
        assert!(preview.warning.unwrap().contains("SKILL.md missing"));
        assert!(preview.body.is_empty());
    }

    #[test]
    fn activate_and_deactivate_reuse_linker() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill_dir = home.join(".claude/skills/greeter");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();
        let skill = resolve_skill("greeter", &home, None).unwrap();
        linker::activate(&project, &home, &skill).unwrap();
        assert!(project.join(".agents/skills/greeter").exists());
        linker::deactivate(&project, &home, "greeter").unwrap();
        assert!(!project.join(".agents/skills/greeter").exists());
        let raw = fs::read_to_string(linker::manifest_path(&project)).unwrap();
        assert!(!raw.contains("greeter") || !raw.contains("[[skills]]") || {
            let m = linker::load_manifest(&project).unwrap();
            !m.skills.iter().any(|s| s.name == "greeter")
        });
    }

    /// Serializes tests that mutate process `HOME` / cwd (same isolation as CLI).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct IsolatedFs {
        _guard: std::sync::MutexGuard<'static, ()>,
        prev_cwd: std::path::PathBuf,
        prev_home: Option<std::ffi::OsString>,
        prev_data: Option<std::ffi::OsString>,
        prev_cfg: Option<std::ffi::OsString>,
    }

    impl IsolatedFs {
        fn enter(home: &Path, data: &Path, cfg: &Path, cwd: &Path) -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev_cwd = std::env::current_dir().expect("cwd");
            let this = Self {
                _guard: guard,
                prev_cwd,
                prev_home: std::env::var_os("HOME"),
                prev_data: std::env::var_os("SKL_DATA_DIR"),
                prev_cfg: std::env::var_os("SKL_CONFIG_DIR"),
            };
            std::env::set_var("HOME", home);
            std::env::set_var("SKL_DATA_DIR", data);
            std::env::set_var("SKL_CONFIG_DIR", cfg);
            std::env::set_current_dir(cwd).expect("chdir project");
            this
        }
    }

    impl Drop for IsolatedFs {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.prev_cwd);
            match &self.prev_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match &self.prev_data {
                Some(v) => std::env::set_var("SKL_DATA_DIR", v),
                None => std::env::remove_var("SKL_DATA_DIR"),
            }
            match &self.prev_cfg {
                Some(v) => std::env::set_var("SKL_CONFIG_DIR", v),
                None => std::env::remove_var("SKL_CONFIG_DIR"),
            }
        }
    }

    fn plant_library_skill(data: &Path, name: &str, body: &str) -> PathBuf {
        let dir = data.join("skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
        dir
    }

    fn dest_real(project: &Path, name: &str) -> PathBuf {
        let link = project.join(".agents/skills").join(name);
        assert!(
            link.exists() || link.symlink_metadata().is_ok(),
            "missing dest {}",
            link.display()
        );
        fs::canonicalize(&link).unwrap_or_else(|_| link)
    }

    /// `u` in the TUI must write the same portable `skills.toml` + dests as `skl use`.
    #[test]
    fn tui_u_matches_skl_use_manifest_and_links() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let data = home.join(".local/share/skl");
        let cfg = home.join(".config/skl");
        let project_tui = tmp.path().join("proj-tui");
        let project_cli = tmp.path().join("proj-cli");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&cfg).unwrap();
        fs::create_dir_all(&project_tui).unwrap();
        fs::create_dir_all(&project_cli).unwrap();
        let lib = plant_library_skill(&data, "greeter", "# hello from greeter\n");

        let _iso = IsolatedFs::enter(&home, &data, &cfg, &project_tui);
        let mut app = App::load().expect("load catalog from library");
        assert_eq!(
            app.selected_row().map(|r| r.name.as_str()),
            Some("greeter"),
            "TUI list must show planted library skill"
        );
        assert_eq!(app.handle_key(key(KeyCode::Char('u'))), Tick::Continue);
        assert!(
            app.status.contains("using greeter"),
            "u status: {}",
            app.status
        );

        let skill = resolve_skill("greeter", &home, Some(&data.join("state.db"))).unwrap();
        linker::activate_with_extras(&project_cli, &home, &skill, &[]).unwrap();

        let tui_toml = fs::read_to_string(linker::manifest_path(&project_tui)).unwrap();
        let cli_toml = fs::read_to_string(linker::manifest_path(&project_cli)).unwrap();
        assert_eq!(
            tui_toml, cli_toml,
            "TUI u vs skl use skills.toml\nTUI:\n{tui_toml}\nCLI:\n{cli_toml}"
        );
        assert!(
            !tui_toml.contains("path ="),
            "portable manifest must not write path=: {tui_toml}"
        );
        assert!(tui_toml.contains("name = \"greeter\""), "{tui_toml}");
        assert_eq!(dest_real(&project_tui, "greeter"), dest_real(&project_cli, "greeter"));
        let tui_dest = dest_real(&project_tui, "greeter");
        let lib_real = fs::canonicalize(&lib).unwrap();
        assert_eq!(tui_dest, lib_real, "dest should point at library skill");
    }

    #[test]
    fn activate_cwd_matches_cli_use_without_app() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let data = home.join(".local/share/skl");
        let cfg = home.join(".config/skl");
        let project_tui = tmp.path().join("proj-tui");
        let project_cli = tmp.path().join("proj-cli");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&cfg).unwrap();
        fs::create_dir_all(&project_tui).unwrap();
        fs::create_dir_all(&project_cli).unwrap();
        plant_library_skill(&data, "greeter", "# hello\n");

        let _iso = IsolatedFs::enter(&home, &data, &cfg, &project_tui);
        let msg = activate_cwd("greeter").expect("TUI activate_cwd");
        assert!(msg.contains("using greeter"), "{msg}");

        let skill = resolve_skill("greeter", &home, Some(&data.join("state.db"))).unwrap();
        linker::activate_with_extras(&project_cli, &home, &skill, &[]).unwrap();

        let tui_toml = fs::read_to_string(linker::manifest_path(&project_tui)).unwrap();
        let cli_toml = fs::read_to_string(linker::manifest_path(&project_cli)).unwrap();
        assert_eq!(tui_toml, cli_toml);
        assert_eq!(dest_real(&project_tui, "greeter"), dest_real(&project_cli, "greeter"));
    }
}
