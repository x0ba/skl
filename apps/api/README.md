# @skl/api

Hono + Postgres API for skl skill sync. Hosted-first. Blobs live in Postgres for M0 (no S3). Storage is opaque and content-addressed so ciphertext can land later without a schema rewrite.

Furnace (CLI + device-approve page) should import `apps/api/src/contracts.ts` (or `@skl/api/contracts`). **All routes are under `/v1`.**

## Routes

| Method | Path | Auth | Request | Response |
| --- | --- | --- | --- | --- |
| `POST` | `/v1/auth/device/code` | public | `{ client_name? }` | `{ device_code, user_code, verification_uri, verification_uri_complete, expires_in, interval }` |
| `POST` | `/v1/auth/device/token` | public | `{ device_code, grant_type: "urn:ietf:params:oauth:grant-type:device_code" }` | `{ access_token, expires_in: null }` or `{ error: "authorization_pending" \| "slow_down" \| "expired_token" \| "access_denied" }` |
| `POST` | `/v1/auth/device/approve` | Clerk JWT | `{ user_code, device_name? }` | `{ ok, device_id }` |
| `GET` | `/v1/devices` | Clerk or device | — | `{ devices: [{ id, name, created_at, last_used_at, revoked_at }] }` |
| `DELETE` | `/v1/devices/:id` | Clerk or device | — | `204` |
| `POST` | `/v1/sync` | Clerk or device | `{ skills: { [name]: { tree_hash, files: { [path]: hash } } } }` | `{ upload: hash[], download: [{ hash, skills, paths }], conflicts: [{ skill, local_tree_hash, remote_tree_hash }], missing_skills: string[] }` |
| `PUT` | `/v1/blobs/:hash` | Clerk or device | raw bytes or `{ content_base64 }` | `{ hash, size }` |
| `GET` | `/v1/blobs/:hash` | Clerk or device | — | `application/octet-stream` |
| `PUT` | `/v1/skills/:name/tree` | Clerk or device | `{ tree_hash, files }` | `{ name, tree_hash, updated_at }` |
| `GET` | `/v1/skills` | Clerk or device | — | `{ skills: [{ name, tree_hash, updated_at }] }` |
| `GET` | `/v1/skills/:name` | Clerk or device | — | `{ name, tree_hash, files, updated_at }` |
| `GET` | `/v1/health` | public | — | `{ ok: true }` |

Device tokens are long-lived (`expires_in: null`). **No `refresh_token`.** Raw token is returned once; only a SHA-256 hash is stored. Sync is hash-map `POST /v1/sync` only (no cursor/etag).

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
| `SKL_WEB_ORIGIN` | no | default `http://localhost:3000` — `verification_uri` + CORS |
| `SKL_API_ORIGIN` | no | default `http://localhost:8787` |
| `CLERK_SECRET_KEY` | prod | verifies Clerk session JWTs |
| `CLERK_PUBLISHABLE_KEY` | prod (web) | not consumed by the API |
| `CLERK_WEBHOOK_SECRET` | later | **TODO:** user.created / user.deleted webhook |
| `ALLOW_DEV_AUTH` | local | on when `CLERK_SECRET_KEY` is unset. `Authorization: Bearer dev:<clerk_user_id>` |

## Demo path

1. CLI `POST /v1/auth/device/code` `{ client_name }` → show `user_code`.
2. Web (Clerk) `POST /v1/auth/device/approve` `{ user_code, device_name? }` → `{ ok, device_id }`.
3. CLI polls `POST /v1/auth/device/token` until `{ access_token, expires_in: null }`.
4. Machine A: `POST /v1/sync` → `upload` hashes → `PUT /v1/blobs/:hash` → `PUT /v1/skills/:name/tree`.
5. Machine B: `POST /v1/sync` with `{}` → `missing_skills` + `download` → `GET /v1/blobs/:hash`.

## Schema

Postgres tables (see `drizzle/0000_init.sql` and `src/db/schema.ts`):

- `users` — `clerk_user_id`
- `devices` — `user_id`, name, `token_hash`, `last_used_at`, created/revoked
- `device_authorizations` — hashed `device_code`, `user_code`
- `skills` / `skill_versions` / `skill_files`
- `blobs` — content-addressed `bytea`

## Scripts

```bash
pnpm dev
pnpm migrate
pnpm test
pnpm typecheck
```
