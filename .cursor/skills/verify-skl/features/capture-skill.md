# Capture a skill

Capture copies a project skill directory into the personal library and indexes it. Unless `--keep-copy`, it then replaces the project dest with a symlink to that library copy and records `mode = "symlink"`.

## Sub-features

- `capture-cli` copies a project tree that contains `SKILL.md` into `{data_dir}/skills/<name>`.
- `capture-symlink` replaces the project dest with a symlink to the library (default).
- `capture-keep-copy` leaves the project dest as a real directory.
- `capture-as` stores the tree under a different library name.
- `capture-clash` refuses an existing library name without `--force` or `--as`.
- `capture-missing-md` refuses a directory with no `SKILL.md`.

## How to get to it (user POV)

- Run `skl capture <path-or-name>` from a project, or pass `--project DIR`.
- Path may be `.agents/skills/<name>` or just `<name>` if that dest exists.

## Driving it with verify-skl

Preconditions:

- Isolate is healthy (`helpers/verify-skl.sh doctor`).
- The isolate project has an unmanaged skill tree `verify-captured` (create the directory and `SKILL.md` in `$VERIFY_SKL_PROJECT/.agents/skills/verify-captured` as a user would: a one-off project skill, not yet in the library).
- No library skill is named `verify-captured`.

- **Capture default.** Run `helpers/verify-skl.sh cli -- capture verify-captured --project "$VERIFY_SKL_PROJECT"`. Exit code `0`. Stderr contains `captured verify-captured` and `$ROOT/data/skills/verify-captured`.
- **Library copy.** `$ROOT/data/skills/verify-captured/SKILL.md` matches the text that was in the project tree. `skl list` shows `verify-captured` with source `agents`.
- **Project dest is now a symlink.** `test -L "$VERIFY_SKL_PROJECT/.agents/skills/verify-captured"` and the link target is `$ROOT/data/skills/verify-captured`. `skills.toml` has `name = "verify-captured"` and `mode = "symlink"`.
- **Already linked.** Run capture again on the same dest. Exit code `0`. Stderr contains `already linked`. Library and dest stay as they are.
- **Keep copy.** In a clean isolate (or a new name), plant `.agents/skills/verify-keep/SKILL.md`, then run `helpers/verify-skl.sh cli -- capture verify-keep --keep-copy --project "$VERIFY_SKL_PROJECT"`. Stderr contains `kept project copy`. Dest is a directory, not a symlink.
- **Rename.** Plant `.agents/skills/temp-name/SKILL.md` and run `helpers/verify-skl.sh cli -- capture temp-name --as verify-renamed --project "$VERIFY_SKL_PROJECT"`. Library path is `$ROOT/data/skills/verify-renamed`.
- **Clash.** After the library already has `verify-captured`, plant a different project tree of the same name and capture without `--force`. Nonzero exit. Stderr contains `already exists`. The library `SKILL.md` is unchanged.
- **Missing SKILL.md.** Run capture on a directory with no `SKILL.md`. Nonzero exit. Stderr contains `missing SKILL.md`.
- **Proof.** Run `helpers/verify-skl.sh evidence capture-skill`. Artifacts include `cli-last.txt`, `library/verify-captured/SKILL.md`, `skills.toml`, and `tree.txt` showing the dest file type (`l` vs `d`).

## Gotchas

- Capture default is the opposite of use default: use copies into the project; capture re-symlinks the project to the library. Cloud-safe capture is `--keep-copy`.
- Capture does not prompt. Clash without `--force` / `--as` is a hard error.
- `skl init` imports from home agent roots, not from the current project. Do not use init as a substitute for capture.
- A dest that already points at the library is a reindex, not a second copy.
- Capture is CLI-only. The TUI has no capture key.
