# Inspect the library

List, status, and doctor are the read path for the personal library, login, and project projections. Doctor warns. It does not rewrite dests.

## Sub-features

- `list-empty` reports no index or an empty local table.
- `list-local` prints name, short tree hash, source, path, and `remote` (`-` when logged out).
- `status-isolate` prints `api_base`, `token`, isolate `config` / `state.db`, `auto_sync`, `local_skills`, `last_sync`.
- `doctor-report` prints the five sections and the linking roles line.
- `doctor-project` describes cwd `skills.toml` modes and warn-only projection issues.
- `tui-browse` shows the same library names in the left pane and `SKILL.md` in the preview.

## How to get to it (user POV)

- Run `skl list`, `skl status`, or `skl doctor`.
- Run bare `skl` on a TTY (or `skl tui` / `skl ui`) and move with `j`/`k` or arrows. Press `/` to filter names, `?` for help.

## Driving it with verify-skl

Preconditions:

- Isolate is healthy (`helpers/verify-skl.sh doctor`).
- For populated list/preview, create `verify-greeter` first ([create-skill](./create-skill.md)).

- **Empty list.** On a fresh launch (no `state.db` yet), run `helpers/verify-skl.sh cli -- list`. Exit code `0`. Stderr is `no local skill index; run \`skl init\` first`. After create has opened `state.db` but before any rows, stdout is `(no local skills)`.
- **Populated list.** After create, run `helpers/verify-skl.sh cli -- list`. Header columns are `name`, `tree_hash`, `source`, `path`, `remote`. `verify-greeter` has source `agents`, path `$ROOT/data/skills/verify-greeter`, `remote` `-`.
- **Status.** Run `helpers/verify-skl.sh cli -- status`. Exit code `0`. Lines include `api_base     http://127.0.0.1:1`, `token        no`, `config       $ROOT/config/config.toml`, `state.db     $ROOT/data/state.db`, `auto_sync    off`, `sync_frequency 900s`, `local_skills` matching the library count, `last_sync    (none)`.
- **Doctor.** Run `helpers/verify-skl.sh doctor` (or `cli -- doctor` from the isolate project). Exit code `0`. Sections `== API`, `== Auth`, `== State`, `== Agent skill roots`, `== Linking`. Health is `unreachable` against `http://127.0.0.1:1`. Token is `absent`. Agent roots under `$ROOT/home` (`.agents/skills`, `.claude/skills`, …) exist or are `missing`. Linking roles mention materialize into `.agents/skills`. No `/workspace/skills.toml`.
- **Doctor after use.** Activate `verify-greeter`, then doctor from `$VERIFY_SKL_PROJECT`. The `project` line includes `verify-greeter=copy`.
- **TUI browse.** `tui-start`. Header matches `library: N skill` / `last sync: never` / `project:`. List title ` skills `. Empty catalog preview contains `No local skills.` and `skl create`. With `verify-greeter`, the left pane shows `· verify-greeter` or `✓ verify-greeter` and the preview title is `verify-greeter / SKILL.md`. Press `/`, type `verify`, the list title becomes ` skills  /verify `. `tui-capture` then `tui-stop`.
- **Proof.** Run `helpers/verify-skl.sh evidence inspect-library`. Artifacts include `status.txt`, `doctor.txt`, and `cli-last.txt`. For TUI browse, include `tui-pane.txt` showing the ` skl ` header.

## Gotchas

- `helpers/verify-skl.sh doctor` already cds to `$VERIFY_SKL_PROJECT`. Bare `skl doctor` from `/workspace` reports this repo's activations and is not isolate proof.
- Doctor may prompt sticky extras on a TTY. The isolate sets `SKL_NO_PROMPT=1` and `CI=1`. Do not answer a prompt that writes `~/.config/skl`.
- Doctor warns only. Cloud-hostile links, dangling dests, and divergent copies stay until the user runs `use --all`, `capture`, or `migrate`.
- `skl list` without `state.db` tells you to `skl init`. Init scans `$HOME` agent roots. In the isolate those roots are empty, so init will not invent `verify-greeter`.
- Logged-out `remote` is `-`. Do not require `yes` without `skl login`.
- Debug builds default `api_base` to `http://localhost:8787`. Launch pins `http://127.0.0.1:1`. A status line that shows localhost:8787 means the isolate config was not loaded.
- TUI browse is offline. `s` (sync) needs login; skip it in this isolate.
