# Unuse a skill

Unuse removes this project's dests for a name and drops the `skills.toml` row. The personal library copy stays.

## Sub-features

- `unuse-copy` deletes a materialized `.agents/skills/<name>` directory when `mode = "copy"`.
- `unuse-link` removes a symlink dest when `mode = "symlink"`.
- `unuse-manifest` drops the name from `skills.toml`.
- `unuse-library` leaves `{data_dir}/skills/<name>` and `skl list` unchanged.
- `unuse-tui` runs the same verb from `U` on the selected row.
- `unuse-unmanaged` refuses to delete a real directory that is not a managed copy.

## How to get to it (user POV)

- Run `skl unuse <skill>` in a project directory, or pass `--project DIR`.
- In the TUI opened from that project, select the skill and press `U`.

## Driving it with verify-skl

Preconditions:

- Isolate is healthy (`helpers/verify-skl.sh doctor`).
- `verify-greeter` is in the library and activated in `$VERIFY_SKL_PROJECT` as `mode = "copy"` (see [use-skill](./use-skill.md)).

- **Unuse copy.** Run `helpers/verify-skl.sh cli -- unuse verify-greeter --project "$VERIFY_SKL_PROJECT"`. Exit code `0`. Stderr contains `unused verify-greeter`, a `removed  agents` line for `.agents/skills/verify-greeter`, and `updated  $VERIFY_SKL_PROJECT/skills.toml`.
- **Dest gone.** `test ! -e "$VERIFY_SKL_PROJECT/.agents/skills/verify-greeter"`.
- **Manifest.** `skl use --project "$VERIFY_SKL_PROJECT"` prints `(no skills activated` or no longer lists `verify-greeter`. If `skills.toml` remains, it has no `name = "verify-greeter"` row.
- **Library intact.** `$ROOT/data/skills/verify-greeter/SKILL.md` still exists. `helpers/verify-skl.sh cli -- list` still shows `verify-greeter` with source `agents`.
- **Unuse link.** Activate with `--link`, then unuse. Stderr `removed  agents`. The symlink is gone. Library unchanged.
- **Already unused.** Run unuse again. Exit code `0`. Stderr `absent` for the dest is fine. Library unchanged.
- **TUI unuse.** Activate first, `tui-start` from `$VERIFY_SKL_PROJECT`, select `verify-greeter`, press `U`. List mark returns to `·`. `tui-capture` then `tui-stop`.
- **Proof.** Run `helpers/verify-skl.sh evidence unuse-skill`. Artifacts show the dest missing, `list` still containing the library skill, and `cli-last.txt` with `unused verify-greeter`.

## Gotchas

- Unuse is not delete. `skl delete` tombstones the remote skill and removes the library dir; it needs a device token. Skip delete in the local isolate.
- A real directory not recorded as `mode = "copy"` is left in place. Do not `rm -rf` it to force a pass.
- Extra dests from `-a claude-code` are removed with the same unuse (`.claude/skills/<name>`).
- `U` in the TUI uses the TUI cwd. Start the session from `$VERIFY_SKL_PROJECT`.
- After unuse, `skl use --all` has nothing to restore unless another name remains in `skills.toml`.
