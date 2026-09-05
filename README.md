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

Reports home agent skill roots (`~/.claude/skills`, `~/.cursor/skills`, `~/.codex/skills` — same list as `skl init`), whether each exists/writable, symlink capability (copy fallback when unavailable), keyring + `SKL_TOKEN`, XDG `config.toml` / `state.db`, and `GET /v1/health`. Warns (does not mutate) if the current project still has an M0 layout — skills linked under `.claude`/`.cursor` without `.agents/skills`.

```bash
# API down is still a successful report (health = unreachable)
cargo run -p skl -- doctor

# Live API
API_BASE=http://localhost:8787 cargo run -p skl -- doctor
```

### use / unuse

Default is **symlink** into the project's **`.agents/skills`** (canonical), plus legacy `.claude/skills` and `.cursor/skills` so M0 projects keep working until `skl migrate targets`. If the filesystem refuses (EPERM / ENOTSUP / Windows privilege), `skl use` copies instead and records `mode = "copy"` in `skills.toml` (same fallback on every destination). Codex is linked only if `~/.codex/skills` exists or the project already has `.codex`. `--project` overrides cwd.

`skills.toml` may list destinations under `[targets]` using ids `agents`, `claude`, `cursor`, `codex` (dotted aliases like `.claude` are accepted). Missing `[targets]` defaults to canonical=`agents` and extra=`claude`+`cursor`.

```bash
# Home skill (or a path already imported by `skl init`)
mkdir -p ~/.claude/skills/greeter
printf '# hello\n' > ~/.claude/skills/greeter/SKILL.md

# From a project directory
cargo run -p skl -- use greeter
ls -l .agents/skills/greeter .claude/skills/greeter .cursor/skills/greeter
cat skills.toml

cargo run -p skl -- use                 # list activated
cargo run -p skl -- unuse greeter

# Explicit project
cargo run -p skl -- use greeter --project /path/to/proj
```

`skl use` refuses to overwrite a real directory it did not create. `skl unuse` removes symlinks and copy-mode dirs it created. Conflict/scrub hooks live in `crates/cli/src/hooks/` (`skl sync --keep-local` / `--keep-remote`).

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
./scripts/smoke-import-sync-use.sh   # init → sync → skl use
./scripts/smoke-clash.sh             # keep-local / keep-remote + scrub

# Boot postgres + apps/api here
START_API=1 ./scripts/smoke-import-sync-use.sh
```
