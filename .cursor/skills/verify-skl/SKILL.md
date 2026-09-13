---
name: verify-skl
description: Drive the skl CLI and TUI the way a user does. Use when proving create, use, capture, unuse, list, status, or doctor against a real skl binary, or when a change might break library or project-projection behavior.
---

# Verify skl

skl is a personal agent-skill library. The write path is the `skl` CLI and the two-pane TUI (`skl` on a TTY, or `skl tui` / `skl ui`). Web (`/skills`, `/devices`) and HTTP `/v1` are secondary: they list, preview, approve devices, and sync. They do not create, edit, activate, or capture. This skill drives the CLI first. Use the TUI recipe only when the map names a TUI entry.

Never drive the operator's real library (`~/.local/share/skl`) or this repo checkout as `--project`. The checkout already has a `skills.toml` for file-pr / frontend-design / greploop / html-communication.

## Launch

skl is a short-lived binary. There is no server to keep alive for CLI proof. Build once, then start each drive in the isolate this helper creates.

```bash
.cursor/skills/verify-skl/helpers/verify-skl.sh launch
# source the printed session if a later shell needs the same run:
source /tmp/skl-verify-$RUN_ID/session.env
```

Ready when stdout contains `ready  version=skl 0.4.2` and `root   /tmp/skl-verify-<id>`. The helper runs `cargo build -p skl` from the repo root and uses `target/debug/skl`. Debug `api_base` defaults to `http://localhost:8787`; launch overrides that with `http://127.0.0.1:1` and writes `[sync] auto = false` so piggyback sync never leaves the isolate.

Isolate layout:

| Path | Role |
|---|---|
| `$ROOT/data` | `SKL_DATA_DIR` — library at `$ROOT/data/skills`, index at `$ROOT/data/state.db` |
| `$ROOT/config` | `SKL_CONFIG_DIR` — `config.toml` |
| `$ROOT/home` | `HOME` so `skl doctor` / `skl init` scan empty agent roots, not `~/.claude` |
| `$ROOT/project` | disposable `--project` (and TUI cwd) |
| `/tmp/skl-verify-evidence/$RUN_ID` | proof artifacts; cleanup does not delete this |

Launch also sets `SKL_NO_TUI=1`, `SKL_NO_PROMPT=1`, `CI=1`, `VISUAL=true`, `EDITOR=true`, and unsets `SKL_TOKEN` / `SKL_TOKEN_FILE`. `true` is the non-interactive editor: `skl create` still writes `SKILL.md` and returns 0 (`editor.rs`; create treats editor failure as a warning).

Teardown is `helpers/verify-skl.sh cleanup` (see Cleanup). Two isolates can run side by side if each has its own `VERIFY_SKL_RUN_ID`. Do not reuse a session you did not start.

## Doctor

Run this first whenever anything looks off. It is read-only besides printing.

```bash
.cursor/skills/verify-skl/helpers/verify-skl.sh doctor
```

The helper requires `skl 0.4.2`, isolated `SKL_DATA_DIR` / `SKL_CONFIG_DIR` / `HOME`, then runs `skl status` and `skl doctor` from `$VERIFY_SKL_PROJECT` so the `project` line is the isolate, not this repo.

Worth driving only when doctor prints `doctor ok` and both reports show:

- `api_base     http://127.0.0.1:1`
- `state.db     $ROOT/data/state.db`
- `config       $ROOT/config/config.toml`
- `auto_sync    off` (status)
- `token        no` / `token        absent` (local-only isolate)
- `== API`, `== Auth`, `== State`, `== Agent skill roots`, `== Linking`
- `roles        personal library is canonical; use materializes (copy) into .agents/skills`
- no `/workspace/skills.toml` on the project line

`skl doctor` always exits 0 after printing (`apps/docs/docs/reference/doctor.mdx`). Health `unreachable` on `http://127.0.0.1:1` is expected. A live `GET /v1/health` is not required for local create/use/capture/unuse.

Refuse to continue if `SKL_DATA_DIR` is unset or is the default XDG path.

## Drive

Prefer the helper. It already exports the isolate and sets cwd to `$VERIFY_SKL_PROJECT`.

```bash
.cursor/skills/verify-skl/helpers/verify-skl.sh cli -- create verify-greeter
.cursor/skills/verify-skl/helpers/verify-skl.sh cli -- use verify-greeter --project "$VERIFY_SKL_PROJECT"
.cursor/skills/verify-skl/helpers/verify-skl.sh cli -- use --project "$VERIFY_SKL_PROJECT"
.cursor/skills/verify-skl/helpers/verify-skl.sh cli -- list
.cursor/skills/verify-skl/helpers/verify-skl.sh cli -- doctor
```

Stable handles (do not use coordinates or tab order):

| Surface | Handle |
|---|---|
| CLI create | stderr `created <name>  (<data>/skills/<name>)` |
| CLI use | stderr `using <name>  (agents  …)` then `copy     agents   <project>/.agents/skills/<name>` then `updated  <project>/skills.toml` |
| CLI use list | stdout header `name` / `source` / `mode`; rows use source `library` and mode `copy` |
| CLI unuse | stderr `unused <name>` then `removed  agents   …` or `absent` |
| CLI capture | stderr `captured <name>  (<data>/skills/<name>)` |
| CLI list | stdout columns `name`, `tree_hash`, `source`, `path`, `remote`; library source is `agents` |
| CLI list empty | stderr `no local skill index; run \`skl init\` first` or stdout `(no local skills)` |
| TUI chrome | header `library: N skill(s)  ·  last sync: never  ·  project: …`; list title ` skills `; empty preview `No local skills.` |
| TUI create | overlay title ` new skill `; hint `Enter opens $EDITOR with name + description frontmatter.` |
| TUI keys | `n` create, `/` search, `u` use, `U` unuse, `e` edit, `d` delete, `s` sync, `r` refresh, `?` help, `q`/`Esc` quit |

TUI (only when a feature file names it). Needs a real TTY. Do not set `SKL_NO_TUI`. `TERM=dumb` degrades (`tui/launch.rs`).

```bash
.cursor/skills/verify-skl/helpers/verify-skl.sh tui-start
tmux -f /exec-daemon/tmux.portal.conf send-keys -t "$VERIFY_SKL_TMUX:0.0" 'n'   # or the key the map names
.cursor/skills/verify-skl/helpers/verify-skl.sh tui-capture
.cursor/skills/verify-skl/helpers/verify-skl.sh tui-stop
```

If `/exec-daemon/tmux.portal.conf` is missing, the helper calls plain `tmux`. Capture the pane after the UI settles; do not assert on a fixed sleep alone.

Read `features/README.md` and the matching feature file before driving. A proof that uses one convenient entry is incomplete when that file lists others.

## Evidence

Write proof under `/tmp/skl-verify-evidence/$RUN_ID/<feature-id>/` via:

```bash
.cursor/skills/verify-skl/helpers/verify-skl.sh evidence use-skill
```

That directory gets `cli-last.txt`, `cli-history.txt` (every `cli --` in the run), `doctor.txt`, `status.txt`, dest trees, and `skills.toml`. Cleanup never deletes it. Copy elsewhere if a reviewer needs a durable attachment; do not commit run artifacts into `.cursor/skills/verify-skl/`.

Proof standards:

- Exercise the real user command (`skl create`, `skl use`, …) or the TUI key that maps to it. Do not insert rows into `state.db` or write `.agents/skills` by hand and call that a pass.
- Capture the action and the resulting state: command + exit code + stderr/stdout, then a second read (`skl list`, `skl use` with no names, `cat` of `SKILL.md` / `skills.toml`, `find` of dests).
- Mutation proof includes on-disk side effects: library tree, dest file type (directory vs symlink), and `skills.toml` `name` / `source` / `mode`.
- `source = "library"` in `skills.toml` is the portable token. Library index rows use source `agents`. Do not "fix" that (`library.rs`).
- Default `use` is a real directory (`mode = "copy"`). `--link` is a symlink (`mode = "symlink"`). Capture without `--keep-copy` rewrites the project dest to a symlink.
- Local-only isolate: skip `skl sync`, `skl login`, and `skl delete` (delete requires a device token and `DELETE /v1/skills/:name`). Report those paths as skipped with the unmet precondition. Do not claim them verified via create/use.
- Auto-sync is disabled in the isolate. If a recipe turns it on, observe network / `last_sync` rather than trusting the name.

## Cleanup

```bash
.cursor/skills/verify-skl/helpers/verify-skl.sh cleanup
```

Stops the tmux session this run started (by session name / pane pid, never `pkill -f skl`), then deletes only `$ROOT` matching `/tmp/skl-verify-*`. Evidence stays at `/tmp/skl-verify-evidence/$RUN_ID`.

After a failed iteration, run the same cleanup before the next launch so the next `tui-start` does not collide.

## Helpers

`helpers/verify-skl.sh` is executable. Every invocation in this file is literal.

| Command | What it does |
|---|---|
| `launch` | `cargo build -p skl`, isolate dirs, session.env, version gate |
| `doctor` | isolate + `skl status` + `skl doctor` from the isolate project |
| `cli -- <args>` | `skl <args>` with isolate env, cwd = isolate project |
| `tui-start` / `tui-capture` / `tui-stop` | one tmux session named `skl-verify-$RUN_ID` |
| `evidence <id>` | copy logs, trees, and skill files into the evidence dir |
| `cleanup` | tear down isolate and tmux; keep evidence |

Reuse a run in another shell with `source /tmp/skl-verify-$RUN_ID/session.env` or `VERIFY_SKL_RUN_ID=...`.

## Maintenance

When commands, dest layout, or TUI keys change, update this skill and `features/` with `/maintain-verification-skill`.
