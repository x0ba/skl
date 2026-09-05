# skl

Atuin-style personal agent skill sync.

## Install

```bash
curl -fsSL https://github.com/x0ba/skl/releases/latest/download/install.sh | bash
```

Detects OS/arch, downloads the matching GitHub Release asset, and installs to `~/.local/bin/skl` (no sudo). The installer **never** edits `.bashrc` / `.zshrc` / fish config — if `~/.local/bin` is not on `PATH` it prints an `export` you can add yourself.

On a TTY it then runs `skl setup`: login `[Y/n]` (default yes), init `[Y/n]` (default yes), then the existing harness checklist (Universal `.agents/skills` locked; extras like `claude-code` are toggleable).

### Non-interactive

Non-TTY (CI, `curl | bash` without a terminal) or `--non-interactive` installs the **binary only** — no login, init, or checklist.

```bash
curl -fsSL https://github.com/x0ba/skl/releases/latest/download/install.sh | bash -s -- --non-interactive

# same:
curl -fsSL https://github.com/x0ba/skl/releases/latest/download/install.sh | SKL_NON_INTERACTIVE=1 bash
skl setup --non-interactive
```

Windows: download `skl-x86_64-pc-windows-gnu.exe` from [Releases](https://github.com/x0ba/skl/releases/latest). No PowerShell installer this milestone.

From source (footnote): `cargo install --path crates/cli` or `cargo build --release -p skl`.

- `apps/api` — Hono + Postgres (auth, device flow, hash-based sync). All HTTP routes are under `/v1`; see `apps/api/src/contracts.ts`.
- `apps/web` — Next.js (furnace): device-approve + dashboard
- `crates/cli` — CLI (furnace). Package/binary name is `skl` (`cargo build -p skl` / `cargo test -p skl`).

## API

Postgres:

```bash
docker compose up -d postgres
```

Then see [`apps/api/README.md`](apps/api/README.md). Contract types: [`apps/api/src/contracts.ts`](apps/api/src/contracts.ts).

## Web

Device-approve (`/device`) and a bare dashboard (`/`). See [`apps/web/README.md`](apps/web/README.md).

```bash
# API on :8787 (CLERK_SECRET_KEY unset → Bearer dev:<user_id>)
# then:
cd apps/web
cp .env.example .env.local
pnpm install
pnpm dev
```

`skl login` opens `http://localhost:3000/device?user_code=…`. Paste the code if needed, approve, then check `/` for the new device.

If `apps/api/.env` sets `ALLOW_DEV_AUTH=false`, local `dev:<user_id>` approve fails with `clerk_not_configured` even when `CLERK_SECRET_KEY` is empty. Leave that flag unset (or `true`).

## CLI

`API_BASE` / `--api-base` defaults to `http://localhost:8787`. Contracts stay `/v1`.

```bash
cargo check -p skl
cargo test -p skl
cargo run -p skl -- --help
```

### TUI

Bare `skl` on a TTY opens a local two-pane skill browser (list + `SKILL.md` preview). Same as `skl tui` / `skl ui`. Startup reads the personal library / `state.db` only — no network.

Non-TTY, `SKL_NO_TUI=1`, or `--no-tui` prints help and never enters raw mode (safe in CI). `skl -h` / `skl help` still show help. `TERM=dumb` and unsupported Windows consoles print a degrade message instead of a half-broken UI.

| Key | Action |
| --- | --- |
| `/` | search (filter names) |
| `↑` `↓` / `j` `k` | move in the list (list owns arrows) |
| `[` `]` or Ctrl-j / Ctrl-k | scroll preview |
| `e` | edit `SKILL.md` (`$VISUAL` or `$EDITOR`) |
| `u` / `U` | use / unuse in **cwd** (same as `skl use` / `skl unuse`) |
| `s` | sync (blocking; same as `skl sync`) |
| `r` | refresh from local library |
| `?` | help |
| `q` / Esc | quit (Esc also leaves search / help) |

Header shows skill count, last sync age, and the cwd project name. Activated skills are marked `✓`, others `·`. Capture, init, collections, and mouse are not in this TUI.

```bash
./scripts/smoke-tui.sh   # no API: piped/CI help, TTY --no-tui, u == skl use, q restores
```

### setup

`skl setup` is the TTY first-run after `install.sh`: login `[Y/n]` (default yes), then init `[Y/n]` (default yes). Init shows the harness checklist (Universal `.agents` locked). `--non-interactive`, non-TTY, `CI`, `SKL_NO_PROMPT`, or `SKL_YES` skip prompts (binary / already-installed CLI only).

```bash
skl setup
skl setup --non-interactive
```

### login

After `skl login`, the device `access_token` is stored in the OS keyring (`service=skl`, `account=device_token`). In headless/CI environments the keyring may not persist across processes — follow-on commands then say `not logged in`. Export `SKL_TOKEN=<access_token>` (overrides the keyring), or use `skl login --dev-user <id>` / `Bearer dev:<id>` when `ALLOW_DEV_AUTH` is on. `skl doctor` already reports keyring + `SKL_TOKEN` presence.

### auto-sync

Verb-triggered piggyback (no daemon). `login` / `init` / `use` / `unuse` / `capture` / `status` call furnace `maybe_run` when due. `doctor` only *displays* `last_sync`.

`~/.config/skl/config.toml` (`SKL_CONFIG_DIR` overrides XDG):

```toml
[sync]
auto = true              # auto_sync — default on; false → explicit `skl sync` only
frequency_secs = 900     # sync_frequency — 15m; also the failed-attempt throttle
```

- **`auto_sync`** — `[sync].auto`. When on, those verbs POST `/v1/sync` if due. Explicit-sync smokes write `auto = false`.
- **`sync_frequency`** — `[sync].frequency_secs` (default `900`). Two rapid verbs inside the window share one network sync. Age is `last_sync` *and* `last_auto_sync_attempt_at`.
- **`last_sync`** — not a config key. Written to `state.db` (`last_sync_at` + summary) and printed by `skl status` / `skl doctor`. A failed piggyback is fail-soft: the verb still succeeds and `skl status` shows `sync_issue   …` (cleared on the next successful sync).

`skl use` with the API down still links a skill already in the local library. Background conflicts use keep-remote (no TTY): a same-slug local edit after the first upload will not push (the conflict restores remote). New skills and remote-only updates still piggyback. Explicit `skl sync --keep-local` is the overwrite path. Explicit `skl sync` still fails hard if the API is unreachable.

```bash
cargo run -p skl -- status   # auto_sync / sync_frequency / last_sync / optional sync_issue
./scripts/smoke-auto-sync.sh # dual-HOME + throttle + fail-soft (no `skl sync`)
```

### doctor

Reports every unique catalog global root (vercel-labs/skills agents.ts) plus `~/.agents/skills` and `~/.config/agents/skills` — same list as `skl init` — whether each exists/writable, symlink capability (copy fallback when unavailable), keyring + `SKL_TOKEN`, XDG `config.toml` / `state.db`, and `GET /v1/health`. Warns only (does not mutate) if the project still has an M0 layout without `.agents/skills` — run `skl migrate targets`. Also warns if `skills.toml` still lists host-absolute `path`s — run `skl use --all` to rewrite names-only. If no sticky extras are set yet and stdin is a TTY, `init`/`doctor` show an interactive checklist (↑↓ move, space toggle, enter confirm): locked Universal (`.agents/skills`) is always on; toggleable rows are detected custom-project agents plus `claude-code`. Never cursor/codex. CI / non-TTY / `SKL_NO_PROMPT` / `SKL_YES` skip the UI.
```bash
# API down is still a successful report (health = unreachable)
cargo run -p skl -- doctor

# Live API
API_BASE=http://localhost:8787 cargo run -p skl -- doctor
```

### Harness catalog

Vendored from [vercel-labs/skills `src/agents.ts`](https://github.com/vercel-labs/skills/blob/main/src/agents.ts) at `crates/cli/data/agents-catalog.json` (~77 ids). Refresh with `node scripts/sync-agents-catalog.mjs`.

- **Universal** — `project_skills_dir == .agents/skills` (cursor, codex, amp, …). `skl use` alone covers these readers; they are **never** extras / `-a` / checklist rows.
- **Custom** — any other project dir (e.g. `claude-code` → `.claude/skills`). Sticky extras, `-a`, and the checklist only accept these.
- **Alias** — sticky / `-a` / `skills.toml` `claude` migrates to `claude-code`. Leftover `cursor` / `codex` extras are dropped with a stderr warn (no-op).
- OpenClaw's dynamic global is baked to `~/.openclaw/skills`. New canonical files prefer `~/.agents/skills`; init still imports every unique catalog global.

### targets

Sticky extra dests live in `~/.config/skl/config.toml` (`SKL_CONFIG_DIR` overrides the XDG config dir). Canonical dest is always `.agents/skills`. Extras are custom catalog ids only (`claude-code`, `windsurf`, …). `skl targets add cursor` (or any universal id) is rejected.

```bash
cargo run -p skl -- targets                    # show canonical + sticky extras
cargo run -p skl -- targets add claude-code    # also link .claude/skills on `use`
cargo run -p skl -- targets add claude         # alias → claude-code
cargo run -p skl -- targets remove claude-code
```

### use / unuse

Default is **symlink** into the project's **`.agents/skills` only** (enough for cursor/codex). Extra harness dirs are created only when the catalog project dir is custom and opted in via:

- sticky extras (`skl targets add claude-code` → `~/.config/skl/config.toml`)
- project `skills.toml` `[targets].extra`
- this invocation: `skl use <skill> -a claude-code`

If the filesystem refuses (EPERM / ENOTSUP / Windows privilege), `skl use` copies instead and records `mode = "copy"` in `skills.toml`. `--project` overrides cwd.

`skills.toml` is **commit-safe across machines**: it lists skill **names** (and `mode`), not absolute paths. Manifest = what; the personal library on this machine (`~/.local/share/skl/skills/`, or `SKL_DATA_DIR`) = where. Project dests (`.agents/skills` and extras) are local materialization — do not commit them. See [`examples/skills.gitignore`](examples/skills.gitignore) (copy lines you want; `skl` will not edit `.gitignore` for you).

After cloning a repo that already has `skills.toml`:

```bash
skl sync          # pull the personal library onto this machine
skl use --all     # rematerialize every listed skill into project dests
```

`skl use` with no args still **lists** activated skills. Restore is only `skl use --all` — sync never auto-restores.

```bash
./scripts/smoke-portable-use-all.sh  # two-HOME: clone portable toml → sync B → use --all

# Home skill (or a path already imported by `skl init`)
mkdir -p ~/.claude/skills/greeter
printf '# hello\n' > ~/.claude/skills/greeter/SKILL.md

# From a project directory — agents-only unless extras are opted in
cargo run -p skl -- use greeter
ls -l .agents/skills/greeter
test ! -e .claude && test ! -e .cursor
cat skills.toml   # names + mode; no host path

# This run only (also persisted on the project as [targets].extra)
cargo run -p skl -- use greeter -a claude-code
ls -l .agents/skills/greeter .claude/skills/greeter

cargo run -p skl -- use                 # list activated
cargo run -p skl -- use --all           # restore all names from this machine's library
cargo run -p skl -- unuse greeter

# Explicit project
cargo run -p skl -- use greeter --project /path/to/proj
```

`skl use` refuses to overwrite a real directory it did not create. `skl unuse` removes symlinks and copy-mode dirs it created. Conflict/scrub hooks live in `crates/cli/src/hooks/` (`skl sync --keep-local` / `--keep-remote`).

### capture

Promote a project-local skill directory into the **personal library** at `{SKL_DATA_DIR}/skills/<name>/` (default `~/.local/share/skl/skills/`). `.agents/skills` is the project link destination (and a home discovery root for `init`) — it is not the library. SQLite index stays in `state.db` under the data dir.

```bash
# From a project that already has .agents/skills/greeter (real copy, with SKILL.md)
cargo run -p skl -- capture .agents/skills/greeter
# copies into ~/.local/share/skl/skills/greeter and replaces the project path with a symlink

cargo run -p skl -- capture .agents/skills/greeter --keep-copy   # leave project as a real copy
cargo run -p skl -- capture .agents/skills/greeter --as notes    # library name `notes`
cargo run -p skl -- capture .agents/skills/greeter --force       # overwrite existing library skill
```

Name clash without `--force` / `--as` errors (non-TTY: no prompt). A project symlink that already points at the library skill is a no-op. After a successful local capture, piggyback `maybe_run` is fail-soft (API down does not fail capture).

```bash
./scripts/smoke-capture.sh # Dual-HOME capture → sync B → use + clash / --force / --as / --keep-copy / fail-soft / non-TTY
```

### migrate targets

Explicit only — `skl use` / `skl doctor` never rewrite an M0 layout. Detects projects whose skills live only under `.claude`/`.cursor`, ensures `.agents/skills/<skill>` from the home library (symlink→copy fallback), and writes `[targets]` (`canonical = ["agents"]`, prior dests as `extra`). Old links stay unless `--prune-old`. Rewrites `skills.toml` to the portable names-only shape (drops legacy absolute `path`).

```bash
# Deliberate M0 fixture (no API):
./scripts/smoke-migrate-targets.sh

cargo run -p skl -- migrate targets --project /path/to/m0-proj
cargo run -p skl -- migrate targets --project /path/to/m0-proj --prune-old
```

### Cross-compile (single binary)

`scripts/cross-compile.sh` produces portable `skl` binaries under `dist/` from `crates/cli`. Host `cargo build --release -p skl` always runs. Linux musl (both arches) and Windows GNU via zig are built when `zig` + `cargo-zigbuild` are present. Apple triples need a macOS runner (`cargo --target` for both `aarch64-apple-darwin` and `x86_64-apple-darwin`). No brew formula.

Release matrix on `v*` tags (same script, published by [`.github/workflows/release.yml`](.github/workflows/release.yml) → [`.github/workflows/cli-binaries.yml`](.github/workflows/cli-binaries.yml); tag pushes are ignored on the path-filtered workflow so only one GitHub Release is created):

- `aarch64-apple-darwin` / `x86_64-apple-darwin`
- `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl`
- `x86_64-pc-windows-gnu`
- plus `SHA256SUMS` and `install.sh`

```bash
# Host binary at minimum. Add musl/windows when tools exist:
./scripts/cross-compile.sh

# Fetch zig + cargo-zigbuild into $HOME/.local, then musl + windows-gnu:
INSTALL_TOOLS=1 ./scripts/cross-compile.sh

# Explicit triples (fails if a requested target cannot be built):
TARGETS=x86_64-unknown-linux-musl,aarch64-unknown-linux-musl ./scripts/cross-compile.sh
```

### Two-machine smoke

Same `ALLOW_DEV_AUTH` user, two `HOME`s, API on `:8787`. Shared helpers live in `scripts/smoke-lib.sh`.

```bash
# API already up (ALLOW_DEV_AUTH=true)
cargo build -p skl
./scripts/smoke-import-sync-use.sh   # init → sync → skl use (.agents/skills first)
./scripts/smoke-clash.sh             # keep-local / keep-remote + scrub
./scripts/smoke-migrate-targets.sh   # M0 fixture → doctor warn → migrate (no API)
./scripts/smoke-init-home-agents.sh  # init from ~/.agents/skills + ~/.config/agents/skills (no API)
./scripts/smoke-auto-sync.sh         # dual-HOME + throttle + fail-soft (no `skl sync`)
./scripts/smoke-capture.sh           # project skill → capture → sync B → use; clash / --force / --as / --keep-copy / fail-soft / non-TTY
./scripts/smoke-portable-use-all.sh  # two-HOME portable skills.toml → sync B → skl use --all
./scripts/smoke-tui.sh               # non-TTY / SKL_NO_TUI / TTY `--no-tui`; TUI `u` == `skl use`; `q` restores cooked
./scripts/smoke-install.sh           # curl install.sh (fake release) → skl --help; no Rust; no prompts

# Boot postgres + apps/api here
START_API=1 ./scripts/smoke-import-sync-use.sh
START_API=1 ./scripts/smoke-auto-sync.sh
START_API=1 ./scripts/smoke-capture.sh
START_API=1 ./scripts/smoke-portable-use-all.sh
```
