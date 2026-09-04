# @skl/api

Hono + Postgres API for skl skill sync. Hosted-first. Blobs live in Postgres for M0 (no S3). Storage is opaque and content-addressed so ciphertext can land later without a schema rewrite.

**All routes are under `/v1`. There are no unversioned aliases.**

Furnace (CLI + device-approve page) should import `apps/api/src/contracts.ts` (or `@skl/api/contracts`).

## Routes

| Method | Path | Auth |
| --- | --- | --- |
| `POST` | `/v1/auth/device/code` | public |
| `POST` | `/v1/auth/device/token` | public (device_code) |
| `POST` | `/v1/auth/device/approve` | Clerk |
| `GET` | `/v1/devices` | Clerk or device |
| `DELETE` | `/v1/devices/:id` | Clerk or device |
| `POST` | `/v1/sync` | Clerk or device |
| `PUT` | `/v1/blobs/:hash` | Clerk or device |
| `GET` | `/v1/blobs/:hash` | Clerk or device |
| `PUT` | `/v1/skills/:name/tree` | Clerk or device |
| `GET` | `/v1/skills` | Clerk or device |
| `GET` | `/v1/skills/:name` | Clerk or device |
| `GET` | `/v1/health` | public |

`GET /` only returns `{ name, prefix: "/v1", health: "/v1/health" }`. It is not an API alias.

## Contracts

See [`src/contracts.ts`](./src/contracts.ts) for request/response types.

Highlights:

- Device token success: `{ access_token, token_type: "Bearer", device_id }`. **No `refresh_token`.** Raw token is returned once; only a SHA-256 hash is stored.
- `POST /v1/sync` conflicts:

  ```ts
  { skill, local_tree_hash, remote_tree_hash, remote_updated_at }
  ```

- `GET /v1/skills` and `GET /v1/skills/:name` include `updated_at`.
- Blobs: `PUT/GET /v1/blobs/:hash` are raw `application/octet-stream`. `:hash` is lowercase hex SHA-256 of the bytes.

## Run locally

From the repo root:

```bash
docker compose up -d postgres
```

In `apps/api`:

```bash
cp .env.example .env
pnpm install
pnpm migrate
pnpm dev
```

API listens on `http://localhost:8787`. Health: `http://localhost:8787/v1/health`.

### Env vars

| Variable | Required | Notes |
| --- | --- | --- |
| `DATABASE_URL` | yes (prod) | default `postgres://skl:skl@localhost:5432/skl` |
| `PORT` | no | default `8787` |
| `SKL_WEB_ORIGIN` | no | default `http://localhost:3000` — used for `verification_uri` + CORS |
| `SKL_API_ORIGIN` | no | default `http://localhost:8787` |
| `CLERK_SECRET_KEY` | prod | verifies Clerk session JWTs on approve + web calls |
| `CLERK_PUBLISHABLE_KEY` | prod (web) | not consumed by the API |
| `CLERK_WEBHOOK_SECRET` | later | **TODO:** user.created / user.deleted webhook (needs Clerk dashboard secret) |
| `ALLOW_DEV_AUTH` | local | defaults on when `CLERK_SECRET_KEY` is unset. Accepts `Authorization: Bearer dev:<clerk_user_id>` |

Local demo without Clerk keys:

```http
POST /v1/auth/device/approve
Authorization: Bearer dev:user_alice
```

## Demo path (import → two machines)

1. CLI: `POST /v1/auth/device/code` → show `user_code`.
2. Web (Clerk): `POST /v1/auth/device/approve` with that `user_code`.
3. CLI polls `POST /v1/auth/device/token` until `{ access_token, token_type, device_id }`.
4. Machine A: `POST /v1/sync` with local `name → tree_hash` (and optional path hashes). Upload set → `PUT /v1/blobs/:hash` then `PUT /v1/skills/:name/tree`.
5. Machine B: `POST /v1/sync` with `{}` → download set → `GET /v1/skills/:name` + `GET /v1/blobs/:hash`.

Hash mismatch with no fast-forward (`base_tree_hash` ≠ remote) is a conflict.

## Schema

Postgres tables (see `drizzle/0000_init.sql` and `src/db/schema.ts`):

- `users` — `clerk_user_id`
- `devices` — `user_id`, name, `token_hash`, created/revoked
- `device_authorizations` — device-code grant (hashed `device_code`, `user_code`)
- `skills` — per-user name/slug + current tree hash + `updated_at`
- `skill_versions` — immutable versions
- `skill_files` — path + content hash per version
- `blobs` — content-addressed `bytea` in Postgres

## Scripts

```bash
pnpm dev        # migrate-on-boot + listen
pnpm migrate    # apply drizzle/*.sql
pnpm test
pnpm typecheck
```

## Production notes

- Run `pnpm migrate` against the production `DATABASE_URL` before serving traffic. `src/index.ts` exports the Hono app for Vercel; it does **not** auto-migrate.
- Set `CLERK_SECRET_KEY`. Do not enable `ALLOW_DEV_AUTH` in production.
- Clerk webhook user sync is intentionally stubbed until `CLERK_WEBHOOK_SECRET` exists.
