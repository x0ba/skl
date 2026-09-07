# skl

A tool to manage and synchronize your personal agent skills across computers and projects. In addition to the CLI, it includes an API and a web interface to manage devices and view skills.

You can self host the web interface and API if you want to! ATM this option is kinda chopped but I'll be adding first-class support for selfhosting soon.

## Features

- Personal skill library; no more scattered skills across global and project-specific `.agents/skills` and `.claude/skills` directories.
- Automatic personal skill syncing across devices.
- Project activation via **materialize (copy)** so Cursor Cloud and other remote agents see real files. `--link` if you want live local symlinks.
- Create skills in the personal library (`skl create` / `n` in the TUI) and edit them with `skl edit` (`e` in the TUI). The library is canonical.
- Capture skills from projects only when the project temporarily won. Everyday loop: `skl edit` → `skl use --all`.
- Supports all major harnesses.
- Interactive TUI to search, preview, edit, activate, and sync without memorizing flags.
- Secret scrubbing that warns/blocks obvious secrets in skills before uploading them to the sync server (still check your skills manually though!)
- `skl update` to replace the CLI with the latest GitHub Release.

## Using skills with Cursor Cloud agents

Cursor Cloud checks out the repo on a remote VM. It does not have your laptop `~/.local/share/skl` library. Absolute symlinks into `$HOME` are dangling there.

`skl use` therefore **copies** (materializes) the skill into `.agents/skills/<name>` by default. Commit those trees plus names-only `skills.toml`.

```bash
skl sync
skl use --all
git add .agents/skills skills.toml && git commit
```

Refresh after a library change with `skl use --all` (or `skl refresh`). Managed projections do not need `--force`. Convert old symlink activations the same way, or `skl migrate projections --materialize`. Local-only live edit: `skl use --link` or `project.projection = "link"` in `~/.config/skl/config.toml`. `skl doctor` warns when a project still uses links or a copy is behind the library.

Do not rely on absolute symlinks for anything remote. skl does not `git add` for you.

## Coming soon

- Teams/org registries
- E2E encrypted sync (TLS + at-rest currently)
- Skill "sets" to group multiple skills into one bundle.




