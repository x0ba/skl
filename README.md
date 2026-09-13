# skl

skl keeps your agent skills in one library on your machine and puts them into projects on demand. A skill is a directory with a `SKILL.md` at its root. The library lives at `~/.local/share/skl/skills`. `skl use <name>` copies a skill from the library into `.agents/skills/<name>` in the current project, where Cursor, Codex, and other agents that read `.agents/skills` pick it up. Log in and `skl sync` keeps the library the same on every machine you use.

skl is for people who already write skills and use more than one machine or more than one agent. It is not a marketplace and it is not git. The web app at [tryskl.fyi](https://tryskl.fyi) lists your skills and devices and approves CLI logins. Every write goes through the CLI or the TUI.

Docs: [docs.tryskl.fyi](https://docs.tryskl.fyi).

## Install

```bash
curl -fsSL https://tryskl.fyi/install.sh | sh
```

The script installs `~/.local/bin/skl` on Linux, macOS, and Windows Git Bash. It does not edit your shell configuration. In a terminal it then asks whether to log in and whether to import skills from directories such as `~/.claude/skills`. Answer `n` to either question to skip it.

To upgrade later, run `skl update`.

## Use it

```bash
skl create writing-tests    # new skill in the library, opens SKILL.md in $EDITOR
cd ~/code/some-project
skl use writing-tests       # copies it to .agents/skills/writing-tests
skl login && skl sync       # optional. Same library on your other machines
```

Run `skl` with no arguments in a terminal to open the TUI. It searches, previews, creates, edits, uses, and syncs skills without flags.

`skl use` writes a real copy, not a symlink, so a remote checkout such as Cursor Cloud sees the files. Commit `.agents/skills` and `skills.toml`. `skills.toml` holds names only. On another machine, `skl sync` then `skl use --all` recreates the copies from that machine's library. Sync never touches a project directory.

Pass `--link` to `skl use` when you want a symlink for local editing. Run `skl capture <name>` to copy a skill you wrote inside a project into the library.

## Develop

The repo is a Cargo workspace plus three Node apps.

| Path | What it is |
|---|---|
| `crates/cli` | The `skl` binary. Rust, clap, SQLite index, ratatui TUI |
| `apps/api` | Hono API on Postgres. Device auth, sync, blobs, skills, devices. `/v1` |
| `apps/web` | Next.js dashboard, device approval page, and the install script at `apps/web/public/install.sh` |
| `apps/docs` | Docusaurus user docs |
| `scripts` | Smoke tests that drive two fake homes against a local API |

Build and test the CLI:

```bash
cargo build -p skl
cargo test -p skl
target/debug/skl doctor
```

Debug builds point `api_base` at `http://localhost:8787`. Release builds point at `https://api.tryskl.fyi`. Override either with `--api-base` or `API_BASE`.

Run the API and web app against a local Postgres:

```bash
docker compose up -d postgres
(cd apps/api && cp .env.example .env && pnpm install && pnpm migrate && pnpm dev)
(cd apps/web && cp .env.example .env.local && pnpm install && pnpm dev)
(cd apps/docs && pnpm install && pnpm start --port 3001)
```

The API listens on `http://localhost:8787`, the web app on `http://localhost:3000`, and the docs on `http://localhost:3001`. With `CLERK_SECRET_KEY` empty, the API accepts `Authorization: Bearer dev:<user_id>`, and `skl login --dev-user <user_id>` stores that token without a device poll.

Run the smoke tests with `API_BASE=http://localhost:8787 ./scripts/smoke-import-sync-use.sh`. Each `scripts/smoke-*.sh` names what it covers. `.cursor/skills/verify-skl` is an agent skill that builds the CLI, launches a throwaway data directory, and drives `create`, `use`, `capture`, `unuse`, `list`, `status`, and `doctor` the way a user does. Never point a smoke test or `--project` at your real library or at this checkout.

To self-host the API and web app, see [Self-host the server](https://docs.tryskl.fyi/how-to/self-host).
