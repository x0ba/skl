# skl verification map

This directory is the maintained source for verifying user-facing skl behavior. Read this index before driving, then use the matching feature file as the recipe.

## Baseline preconditions

- Launch with `.cursor/skills/verify-skl/helpers/verify-skl.sh launch` so `SKL_DATA_DIR`, `SKL_CONFIG_DIR`, and `HOME` are under `/tmp/skl-verify-<id>`.
- Copy the printed `source /tmp/skl-verify-<id>/session.env` line into a later shell, or run helper commands in a fresh shell (they read `/tmp/skl-verify-latest`).
- Run `.cursor/skills/verify-skl/helpers/verify-skl.sh doctor` and require `skl 0.4.2`, isolate `state.db` / `config.toml`, `auto_sync off`, and no `/workspace/skills.toml`.
- `VERIFY_SKL_BIN` is `target/debug/skl`.
- Drive through `helpers/verify-skl.sh cli --` or the TUI tmux session. Never `cd` to the skl checkout and run bare `skl use`.
- `VISUAL=true` / `EDITOR=true` unless a recipe opens a real editor.
- Never drive an instance whose `SKL_DATA_DIR` you did not create.

## Driving conventions

- Start every recipe from the baseline isolate unless its preconditions say otherwise.
- Treat every command as literal. Keep skill names, flags, and overlay titles unchanged.
- CLI is the primary harness. TUI keys are the same verbs (`n` = create, `u` = use, `U` = unuse).
- After a mutation, read the library path and a second user-facing view (`skl list` or `skl use` with no names).
- Restore isolate state with `cleanup`, then a new `launch` if the next recipe needs a clean library. Do not delete `/tmp/skl-verify-evidence`.

## Proof and skip reporting

- Capture the user action and the resulting state, not only the last line of stderr.
- CLI proof includes the command, stdout, stderr, and exit code. The helper writes `logs/cli-last.txt` and appends the full stream to `logs/cli-history.txt`.
- Mutation proof includes a second read of the stored tree (`SKILL.md`, dest file type, `skills.toml`).
- TUI proof includes a pane capture that shows the ` skl ` header and the skill name.
- Record the feature ID and entry point with `helpers/verify-skl.sh evidence <feature-id>`.
- Report an unreachable path with the attempted command and the unmet precondition.
- Do not report a skipped TUI or sync entry as verified through the CLI.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with verify-skl` starts with `Preconditions:` and uses labeled bullets that pair each user action with an exact command and observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable handles, required state, commands, and observable proof.

## Features

- [Create a skill](./create-skill.md) covers CLI and TUI create, name clash, and the starter `SKILL.md`.
- [Use a skill](./use-skill.md) covers materialize, `--link`, list activations, extras, and `--all`.
- [Capture a skill](./capture-skill.md) covers promoting a project tree into the library and the default re-symlink.
- [Unuse a skill](./unuse-skill.md) covers dropping dests and `skills.toml` while leaving the library intact.
- [Inspect the library](./inspect-library.md) covers `skl list`, `skl status`, and `skl doctor`.
