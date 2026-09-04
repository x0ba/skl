# skl

Atuin-style personal agent skill sync.

- `apps/api` — Hono + Postgres (auth, device flow, hash-based sync). Unversioned paths; see `apps/api/src/contracts.ts`.
- `apps/web` — Next.js (furnace)
- `crates/cli` — CLI (furnace)

## API

Postgres:

```bash
docker compose up -d postgres
```

Then see [`apps/api/README.md`](apps/api/README.md). Contract types: [`apps/api/src/contracts.ts`](apps/api/src/contracts.ts).
