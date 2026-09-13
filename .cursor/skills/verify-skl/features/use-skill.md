# Use a skill

Use copies (materializes) a personal-library skill into a project as `.agents/skills/<name>` and records a names-only `skills.toml` row. `--link` writes a symlink instead. `skl use` with no names lists activations. `skl use --all` rematerializes every listed name from this machine's library.

## Sub-features

- `use-copy` materializes a real directory and writes `mode = "copy"`.
- `use-list` lists activated names, source `library`, and mode.
- `use-link` writes a symlink and `mode = "symlink"`.
- `use-all` rematerializes every `skills.toml` name from the library.
- `use-extra` with `-a claude-code` also writes `.claude/skills/<name>` (not `-a cursor` or `-a codex`).
- `use-missing` refuses a name that is not in this machine's library.
- `use-tui` activates the selected row with `u`.

## How to get to it (user POV)

- Run `skl use <skill>` in a project directory, or pass `--project DIR`.
- Run `skl use` with no names to list activations.
- Run `skl use --all` after a library edit or after sync on another machine.
- Run `skl use --link <skill>` for a local live symlink.
- In the TUI opened from that project, select the skill and press `u`.

## Driving it with verify-skl

Preconditions:

- Isolate is healthy (`helpers/verify-skl.sh doctor`).
- Library contains `verify-greeter` from [create-skill](./create-skill.md) (or create it now).
- `$VERIFY_SKL_PROJECT` has no `skills.toml` and no `.agents/skills/verify-greeter`.

- **Materialize.** Run `helpers/verify-skl.sh cli -- use verify-greeter --project "$VERIFY_SKL_PROJECT"`. Exit code `0`. Stderr contains `using verify-greeter`, a `copy     agents` line whose path ends in `.agents/skills/verify-greeter`, and `updated  $VERIFY_SKL_PROJECT/skills.toml`.
- **Dest is a directory.** `test -d "$VERIFY_SKL_PROJECT/.agents/skills/verify-greeter"` and `test ! -L "$VERIFY_SKL_PROJECT/.agents/skills/verify-greeter"`. `SKILL.md` matches the library file.
- **Manifest.** `skills.toml` starts with `# Managed by \`skl use\`` and contains:

  ```
  [[skills]]
  name = "verify-greeter"
  source = "library"
  mode = "copy"
  ```

- **List activations.** Run `helpers/verify-skl.sh cli -- use --project "$VERIFY_SKL_PROJECT"`. Exit code `0`. Stdout is a table with `verify-greeter`, `library`, `copy`.
- **Idempotent refresh.** Run the same `use verify-greeter --project` again. Exit code `0`. Dest stays a directory. Mode stays `copy`.
- **Missing name.** Run `helpers/verify-skl.sh cli -- use does-not-exist --project "$VERIFY_SKL_PROJECT"`. Nonzero exit. Stderr contains `is not in the personal library on this machine`.
- **Rejected extra.** Run `helpers/verify-skl.sh cli -- use verify-greeter --project "$VERIFY_SKL_PROJECT" -a cursor`. Nonzero exit. Cursor already reads `.agents/skills`.
- **Extra dest.** Run `helpers/verify-skl.sh cli -- use verify-greeter --project "$VERIFY_SKL_PROJECT" -a claude-code`. Exit code `0`. `$VERIFY_SKL_PROJECT/.claude/skills/verify-greeter` exists as a copy. `skills.toml` has `extra = ["claude-code"]` under `[targets]`.
- **Link escape.** In a second isolate project (or after `unuse`), run `helpers/verify-skl.sh cli -- use verify-greeter --link --project "$VERIFY_SKL_PROJECT"`. Stderr `link     agents`. Dest is a symlink to `$ROOT/data/skills/verify-greeter`. Manifest `mode = "symlink"`.
- **`--all`.** After a library `SKILL.md` edit, run `helpers/verify-skl.sh cli -- use --all --project "$VERIFY_SKL_PROJECT"`. Exit code `0`. The dest `SKILL.md` matches the library. Combining `--all` with a skill name errors: `use either \`skl use --all\` or \`skl use <skill>\``.
- **TUI use.** `tui-start` from `$VERIFY_SKL_PROJECT`, select `verify-greeter`, press `u`. Toast / status includes `using`. List mark becomes `✓`. `tui-capture` then `tui-stop`.
- **Proof.** Run `helpers/verify-skl.sh evidence use-skill`. Artifacts include `cli-last.txt`, `skills.toml`, `project-agents/verify-greeter/SKILL.md`, and `tree.txt` showing a real directory (or a symlink for the `--link` entry).

## Gotchas

- Default is copy so Cursor Cloud sees bytes. A symlink dest is only correct for the `--link` entry.
- `skl use` with no names lists; it does not activate.
- `--project` is required if cwd is the skl repo. The helper already cds to the isolate project; still pass `--project` when the recipe names it.
- `-a cursor` and `-a codex` are rejected. `claude` is accepted as `claude-code`.
- Use resolves from the personal library only. A dest that exists in the project but not in `$ROOT/data/skills` is `use-missing`.
- Sync never writes dests. After a remote pull you still need `use --all`. That path needs login; skip it in the local isolate.
- Missing `mode` in an old `skills.toml` reads as `symlink`. New `use` writes `copy`.
- `u` in the TUI uses the TUI cwd (the isolate project). Do not start the TUI from `/workspace`.
