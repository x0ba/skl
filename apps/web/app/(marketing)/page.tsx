import Link from "next/link";
import { InstallCommand } from "@/components/ui/install-command";
import { Heading, Label } from "@/components/ui/text";
import { Transcript } from "@/components/ui/transcript";
import { TuiFrame } from "@/components/ui/tui-frame";

const DEMO_SKILLS = [
  {
    name: "writing-tests",
    used: true,
    preview: `---
name: writing-tests
description: Failing test first.
---

Write the failing test first. Name it after
the behavior, not the function.
One assertion per case. No shared
mutable fixtures.`,
  },
  {
    name: "code-review",
    used: false,
    preview: `---
name: code-review
description: Review the diff, not the author.
---

Read the change as a user would.
Ask what breaks if this ships.
Skip style nits the linter already
covers.`,
  },
  {
    name: "commit-messages",
    used: false,
    preview: `---
name: commit-messages
description: Subject line is the why.
---

One logical change per commit.
Subject under 72 characters.
Body says why, not what git already
shows.`,
  },
  {
    name: "secret-scrub",
    used: false,
    preview: `---
name: secret-scrub
description: Block obvious secrets on sync.
---

Scan SKILL.md and sidecars before
upload. Warn on keys, tokens, and
PEM blocks. Still read the files
yourself.`,
  },
] as const;

const DEMO_KEYS =
  "/ search  n new  ↑↓/jk move  [] scroll  e edit  u/U use  d delete  s sync  q quit";

const STEPS = [
  {
    n: "01",
    title: "Personal skill library",
    body: "One single source of truth for your skill files synced across all your libraries.",
    command: "skl list",
  },
  {
    n: "02",
    title: "Declarative per-project skills",
    body: ".toml-backed per-project skill configs.",
    command: "skl use",
  },
  {
    n: "03",
    title: "TUI and web app",
    body: "Fully-featured TUI and web application to browse, manage, and sync your skill library.",
    command: "skl tui",
  },
];

export default function LandingPage() {
  return (
    <>
      <section className="mx-auto w-full max-w-content px-6 pb-20 pt-16 sm:pt-24">
        <h1 className="max-w-xl text-balance font-sans text-display font-bold text-foreground">
          Your skills, on every machine.
        </h1>
        <p className="mt-6 max-w-md text-[17px] leading-relaxed text-muted-foreground">
          Seamlessly sync and manage your agent skills across machines and projects.
        </p>
        <div className="mt-9">
          <InstallCommand />
        </div>
        <p className="mt-4 font-mono text-[12px] text-faint">
          available for macOS, Linux, and Windows
        </p>

        <div className="mt-10 space-y-6">
          <TuiFrame
            lastSync="2m ago"
            project="skl"
            skills={DEMO_SKILLS}
            initialSelected="writing-tests"
            keys={DEMO_KEYS}
            caption="skl"
          />
          <Transcript
            caption="Current project"
            lines={[
              { kind: "command", text: "skl use writing-tests" },
              { kind: "output", text: "→ .agents/skills/writing-tests" },
            ]}
          />
        </div>
      </section>

      <section className="border-t border-border">
        <div className="mx-auto w-full max-w-content px-6 py-16">
          <Label className="mb-8">Killer Features</Label>
          <ol className="border-t border-border">
            {STEPS.map((step) => (
              <li
                key={step.n}
                className="grid gap-x-8 gap-y-4 border-b border-border py-8 md:grid-cols-[3rem_minmax(0,22rem)_minmax(0,1fr)]"
              >
                <Label className="pt-1">{step.n}</Label>
                <div>
                  <h3 className="font-sans text-[17px] font-semibold text-foreground">
                    {step.title}
                  </h3>
                  <p className="mt-2 text-[14px] leading-relaxed text-muted-foreground">
                    {step.body}
                  </p>
                </div>
                <code className="self-start font-mono text-[13px] text-faint md:justify-self-end">
                  <span className="select-none">$ </span>
                  {step.command}
                </code>
              </li>
            ))}
          </ol>
        </div>
      </section>

      <section className="border-t border-border">
        <div className="mx-auto grid w-full max-w-content items-center gap-4 px-6 py-9 md:grid-cols-[minmax(0,12rem)_minmax(0,1fr)_auto] md:gap-x-8">
          <Heading>Get started</Heading>
          <InstallCommand />
          <a
            href="https://docs.tryskl.fyi/guide/getting-started"
            className="font-mono text-[13px] text-primary underline decoration-from-font underline-offset-2 hover:decoration-2"
          >
            Read the docs if you want more detail
          </a>
        </div>
      </section>

      <section className="border-t border-border">
        <div className="mx-auto w-full max-w-content px-6 py-20">
          <h2 className="max-w-lg text-balance font-sans text-[31px] font-bold tracking-[-0.035em] text-foreground">
            Stop copy-pasting skill folders between machines.
          </h2>
          <p className="mt-4 max-w-md text-[15px] leading-relaxed text-muted-foreground">
            Install the CLI and stop worrying about where your skill files are.
          </p>
          <div className="mt-8 flex flex-wrap items-center gap-x-8 gap-y-3 font-mono text-[13px]">
            <Link
              href="/skills"
              className="text-primary underline decoration-from-font underline-offset-2 hover:decoration-2"
            >
              Open the dashboard
            </Link>
            <a
              href="https://github.com/x0ba/skl"
              className="text-muted-foreground underline decoration-from-font underline-offset-2 hover:text-foreground"
            >
              Read the source
            </a>
          </div>
        </div>
      </section>
    </>
  );
}
