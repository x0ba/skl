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

`skl use` refuses to overwrite a real directory that is not a symlink. Hammer conflict/scrub stay TODO hooks in `crates/cli/src/hooks/`.
