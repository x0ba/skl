# Create a skill

Create writes a named tree into the personal library, indexes it, and opens `SKILL.md` in `$VISUAL` or `$EDITOR`. It does not touch project dests.

## Sub-features

- `create-cli` writes `{data_dir}/skills/<name>/SKILL.md` with name + description frontmatter.
- `create-index` shows the new name on `skl list` with source `agents`.
- `create-tui` opens the ` new skill ` overlay from `n` and creates the same tree on Enter.
- `create-clash` refuses a name that already exists in the library.
- `create-invalid` rejects empty names, leading `.`, `/`, or `\`.

## How to get to it (user POV)

- Run `skl create <skill>` or `skl new <skill>` in a terminal.
- In the TUI, press `n`, type a name, press Enter.

## Driving it with verify-skl

Preconditions:

- Isolate is healthy (`helpers/verify-skl.sh doctor`).
- No library skill is named `verify-greeter`.
- `VISUAL` / `EDITOR` are `true`.

- **CLI create.** Run `helpers/verify-skl.sh cli -- create verify-greeter`. Exit code `0`. Stderr contains `created verify-greeter` and `$ROOT/data/skills/verify-greeter`.
- **Starter file.** Read `$ROOT/data/skills/verify-greeter/SKILL.md`. Body is exactly:

  ```
  ---
  name: verify-greeter
  description:
  ---

  ```

- **Indexed.** Run `helpers/verify-skl.sh cli -- list`. Exit code `0`. Stdout has a `verify-greeter` row whose `source` is `agents` and `path` is `$ROOT/data/skills/verify-greeter`. `remote` is `-`.
- **Project untouched.** `$VERIFY_SKL_PROJECT/.agents/skills/verify-greeter` does not exist. `$VERIFY_SKL_PROJECT/skills.toml` does not exist.
- **Clash.** Run `helpers/verify-skl.sh cli -- create verify-greeter` again. Nonzero exit. Stderr contains `already exists`. The existing `SKILL.md` is unchanged.
- **Invalid name.** Run `helpers/verify-skl.sh cli -- create ../etc`. Nonzero exit. Stderr contains `invalid skill name`. No child appears under `$ROOT/data/skills`.
- **TUI create.** From a clean isolate (or a free name `verify-tui`), run `helpers/verify-skl.sh tui-start`. Send `n`. The overlay title is ` new skill ` and the hint is `Enter opens $EDITOR with name + description frontmatter.` Type `verify-tui`, press Enter. After the editor returns, the list shows `· verify-tui` (or `✓` if the TUI cwd already activated it). Run `helpers/verify-skl.sh tui-capture` then `tui-stop`.
- **Proof.** Run `helpers/verify-skl.sh evidence create-skill`. Artifacts include `cli-last.txt`, `library/verify-greeter/SKILL.md`, and `tree.txt` listing the library path. The isolate project dest is absent.

## Gotchas

- Without `VISUAL`/`EDITOR`, create blocks in `vi`. The isolate sets `true`. A failing editor still leaves the files; create exits 0 and prints `edit:`.
- Create is library-only. A pass that only checks stderr without `SKILL.md` + `skl list` is incomplete.
- Library index source is `agents`, not `library`. `library` appears later in `skills.toml` after `use`.
- `n` in the TUI while search (`/`) is open types into the filter, not create. Esc first.
- Name clash is an error. There is no `--force` on create. Capture is the overwrite path.
- Auto-sync after create is off in this isolate. Do not treat a missing remote row as a create failure.
