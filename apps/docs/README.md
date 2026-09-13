# skl docs

User-facing documentation for skl. Docusaurus, served at the site root of `https://docs.tryskl.fyi`.

```bash
cd apps/docs
pnpm install
pnpm start --port 3001
```

`pnpm start` serves the site with live reload. `pnpm build` writes static files to `apps/docs/build` and fails on a broken internal link. Node 20 or newer is required.

Pages live under `docs/` in one folder per Diátaxis mode: `tutorial/`, `how-to/`, `reference/`, and `explanation/`. Keep each page in one mode. `sidebars.ts` lists every page.
