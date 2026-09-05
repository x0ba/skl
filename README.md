# skl

Atuin-style personal agent skill sync.

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

### login

After `skl login`, the device `access_token` is stored in the OS keyring (`service=skl`, `account=device_token`). In headless/CI environments the keyring may not persist across processes — follow-on commands then say `not logged in`. Export `SKL_TOKEN=<access_token>` (overrides the keyring), or use `skl login --dev-user <id>` / `Bearer dev:<id>` when `ALLOW_DEV_AUTH` is on. `skl doctor` already reports keyring + `SKL_TOKEN` presence.

### doctor

Reports home agent skill roots (`~/.claude/skills`, `~/.cursor/skills`, `~/.codex/skills` — same list as `skl init`), whether each exists/writable, symlink capability (copy fallback when unavailable), keyring + `SKL_TOKEN`, XDG `config.toml` / `state.db`, and `GET /v1/health`. Warns only (does not mutate) if the project still has an M0 layout without `.agents/skills` — run `skl migrate targets`. If no sticky extras are set yet and stdin is a TTY, `init`/`doctor` soft-prompt once for extra dests (`claude` / `cursor` / `codex`); CI / non-interactive skips the prompt.
```bash
# API down is still a successful report (health = unreachable)
cargo run -p skl -- doctor

# Live API
API_BASE=http://localhost:8787 cargo run -p skl -- doctor
```

### targets

Sticky extra dests live in `~/.config/skl/config.toml` (`SKL_CONFIG_DIR` overrides the XDG config dir). Canonical dest is always `.agents/skills`. Ids: `agents` (always), extras `claude` | `cursor` | `codex`.

```bash
cargo run -p skl -- targets              # show canonical + sticky extras
cargo run -p skl -- targets add claude   # also link .claude/skills on `use`
cargo run -p skl -- targets add cursor
cargo run -p skl -- targets remove cursor
```

### use / unuse

Default is **symlink** into the project's **`.agents/skills` only**. Extra harness dirs (`.claude/skills`, `.cursor/skills`, `.codex/skills`) are created only when opted in via:

- sticky extras (`skl targets add <id>` → `~/.config/skl/config.toml`)
- project `skills.toml` `[targets].extra`
- this invocation: `skl use <skill> -a claude -a cursor`

If the filesystem refuses (EPERM / ENOTSUP / Windows privilege), `skl use` copies instead and records `mode = "copy"` in `skills.toml`. `--project` overrides cwd.

```bash
# Home skill (or a path already imported by `skl init`)
mkdir -p ~/.claude/skills/greeter
printf '# hello\n' > ~/.claude/skills/greeter/SKILL.md

# From a project directory — agents-only unless extras are opted in
cargo run -p skl -- use greeter
ls -l .agents/skills/greeter
test ! -e .claude && test ! -e .cursor
cat skills.toml

# This run only (also persisted on the project as [targets].extra)
cargo run -p skl -- use greeter -a claude
ls -l .agents/skills/greeter .claude/skills/greeter

cargo run -p skl -- use                 # list activated
cargo run -p skl -- unuse greeter

# Explicit project
cargo run -p skl -- use greeter --project /path/to/proj
```

`skl use` refuses to overwrite a real directory it did not create. `skl unuse` removes symlinks and copy-mode dirs it created. Conflict/scrub hooks live in `crates/cli/src/hooks/` (`skl sync --keep-local` / `--keep-remote`).

### migrate targets

Explicit only — `skl use` / `skl doctor` never rewrite an M0 layout. Detects projects whose skills live only under `.claude`/`.cursor`, ensures `.agents/skills/<skill>` from the home library (symlink→copy fallback), and writes `[targets]` (`canonical = ["agents"]`, prior dests as `extra`). Old links stay unless `--prune-old`.

```bash
# Deliberate M0 fixture (no API):
./scripts/smoke-migrate-targets.sh

cargo run -p skl -- migrate targets --project /path/to/m0-proj
cargo run -p skl -- migrate targets --project /path/to/m0-proj --prune-old
```

### Cross-compile (single binary)

`scripts/cross-compile.sh` produces one portable `skl` binary under `dist/` from `crates/cli`. Host `cargo build --release -p skl` always runs. Linux musl (and Windows GNU via zig) are built when `zig` + `cargo-zigbuild` are present — no brew formula.

```bash
# Host binary at minimum. Add musl/windows when tools exist:
./scripts/cross-compile.sh

# Fetch zig + cargo-zigbuild into $HOME/.local, then musl + windows-gnu:
INSTALL_TOOLS=1 ./scripts/cross-compile.sh

# Explicit triples (fails if a requested target cannot be built):
TARGETS=x86_64-unknown-linux-musl ./scripts/cross-compile.sh
```

CI workflow: [`.github/workflows/cli-binaries.yml`](.github/workflows/cli-binaries.yml).

### Two-machine smoke

Same `ALLOW_DEV_AUTH` user, two `HOME`s, API on `:8787`. Shared helpers live in `scripts/smoke-lib.sh`.

```bash
# API already up (ALLOW_DEV_AUTH=true)
cargo build -p skl
./scripts/smoke-import-sync-use.sh   # init → sync → skl use (.agents/skills first)
./scripts/smoke-clash.sh             # keep-local / keep-remote + scrub
./scripts/smoke-migrate-targets.sh   # M0 fixture → doctor warn → migrate (no API)

# Boot postgres + apps/api here
START_API=1 ./scripts/smoke-import-sync-use.sh
```
