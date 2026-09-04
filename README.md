# skl

Atuin-style personal agent skill sync.

- `apps/api` — Hono + Postgres (auth, device flow, hash-based sync). All HTTP routes are under `/v1`; see `apps/api/src/contracts.ts`.
- `apps/web` — Next.js (furnace): device-approve + dashboard
- `crates/cli` — CLI (furnace)

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

### doctor

Reports home agent skill roots (`~/.claude/skills`, `~/.cursor/skills`, `~/.codex/skills` — same list as `skl init`), whether each exists/writable, keyring + `SKL_TOKEN`, XDG `config.toml` / `state.db`, and `GET /v1/health`.

```bash
# API down is still a successful report (health = unreachable)
cargo run -p skl -- doctor

# Live API
API_BASE=http://localhost:8787 cargo run -p skl -- doctor
```

### use / unuse

Default is **symlink** (not copy) into the project's `.claude/skills` and `.cursor/skills`. Codex is linked only if `~/.codex/skills` exists or the project already has `.codex`. Writes/updates project `skills.toml`. `--project` overrides cwd.

```bash
# Home skill (or a path already imported by `skl init`)
mkdir -p ~/.claude/skills/greeter
printf '# hello\n' > ~/.claude/skills/greeter/SKILL.md

# From a project directory
cargo run -p skl -- use greeter
ls -l .claude/skills/greeter .cursor/skills/greeter
cat skills.toml

cargo run -p skl -- use                 # list activated
cargo run -p skl -- unuse greeter

# Explicit project
cargo run -p skl -- use greeter --project /path/to/proj
```

`skl use` refuses to overwrite a real directory that is not a symlink. Conflict/scrub hooks live in `crates/cli/src/hooks/` (`skl sync --keep-local` / `--keep-remote`).

### Cross-compile (single binary)

`scripts/cross-compile.sh` produces one portable `skl` binary under `dist/`. Host `cargo build --release` always runs. Linux musl (and Windows GNU via zig) are built when `zig` + `cargo-zigbuild` are present — no brew formula.

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
