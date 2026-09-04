# skl web

Next.js App Router UI for M0 furnace: **device approve** + a **bare dashboard**.

API calls go to `NEXT_PUBLIC_API_BASE` (default `http://localhost:8787`) and keep the `/v1` prefix from `apps/api/src/contracts.ts`.

| Page | Route |
| --- | --- |
| Dashboard (skill count + devices) | `/` |
| Approve CLI device | `/device` (`?user_code=` prefill, matches API `verification_uri`) |
| Clerk sign-in / sign-up | `/sign-in`, `/sign-up` |

## Run locally

From the repo root, start Postgres + the API first (see [`apps/api/README.md`](../api/README.md)):

```bash
docker compose up -d postgres
```

```bash
# apps/api
cp .env.example .env
pnpm install
pnpm migrate
pnpm dev
```

Leave `CLERK_SECRET_KEY` empty in `apps/api/.env` so the API accepts
`Authorization: Bearer dev:<clerk_user_id>`.

Then the web app:

```bash
# apps/web
cp .env.example .env.local
pnpm install
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000). The Bearer token field
defaults to `dev:local-dev` (override with `NEXT_PUBLIC_DEV_USER_ID`).

### Clerk (optional)

Set `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` and `CLERK_SECRET_KEY` in
`apps/web/.env.local`. Use the same `CLERK_SECRET_KEY` on the API so it can
verify session JWTs. The dashboard and `/device` then call
`getToken()` and send `Authorization: Bearer <clerk_session_jwt>`.

If Clerk keys are missing, the pages still work with a pasted session JWT or
the local `dev:<user_id>` token.

## Approve a device against the local API

1. Start API (`:8787`) and web (`:3000`) as above.
2. From the repo root, start device login:

   ```bash
   cargo run -p skl -- login
   ```

   The CLI prints a `user_code` and a `verification_uri_complete` like
   `http://localhost:3000/device?user_code=ABCD-2345`.

   Without the CLI, mint a code directly:

   ```bash
   curl -s -X POST http://localhost:8787/v1/auth/device/code \
     -H 'content-type: application/json' \
     -d '{"client_name":"laptop"}'
   ```

3. Open `/device` (or the printed URI). Confirm the Bearer token is
   `dev:local-dev` (or sign in with Clerk). Paste the `user_code` if it was
   not prefilled. Submit **Approve device**.
4. The page calls `POST /v1/auth/device/approve` with
   `{ user_code, device_name? }` and shows `{ ok, device_id }` on success.
   Invalid codes return 404; expired codes are shown as expired (API
   `expired_token` / 410).
5. The CLI poll of `POST /v1/auth/device/token` should then receive
   `{ access_token, expires_in: null }`.
6. `/` lists the new device via `GET /v1/devices` and the skill count via
   `GET /v1/skills`. **Revoke** calls `DELETE /v1/devices/:id`.

## Scripts

```bash
pnpm dev
pnpm build
pnpm lint
```
