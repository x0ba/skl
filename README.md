# skl

A tool to manage and synchronize your personal agent skills across computers and projects. In addition to the CLI, it includes an API and a web interface to manage devices and view skills.

You can self host the web interface and API if you want to! ATM this option is kinda chopped but I'll be adding first-class support for selfhosting soon.

## Features

- Personal skill library; no more scattered skills across global and project-specific `.agents/skills` and `.claude/skills` directories.
- Automatic personal skill syncing across devices.
- Declarative symlinking of skills to projects.
- Capture skills from projects to add once-temporary skills to your personal skill library.
- Supports all major harnesses.
- Interactive TUI to search, preview, edit, activate, and sync without memorizing flags.
- Secret scrubbing that warns/blocks obvious secrets in skills before uploading them to the sync server (still check your skills manually though!)

## Coming soon

- Teams/org registries
- E2E encrypted sync (TLS + at-rest currently)
- Skill "sets" to group multiple skills into one easily symlinkable bundle.




