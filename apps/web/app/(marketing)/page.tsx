import Link from "next/link";
import { InstallCommand } from "@/components/ui/install-command";
import { Heading, Label } from "@/components/ui/text";
import { Transcript } from "@/components/ui/transcript";

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
        <div className="grid gap-14 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] lg:items-center lg:gap-16">
          <div>
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
          </div>

          <div className="space-y-6">
            <Transcript
              caption="First run on a new machine"
              lines={[
                { kind: "command", text: "skl login" },
                { kind: "output", text: "code: BQDF-7T2M" },
                { kind: "output", text: "open https://tryskl.fyi/device" },
                { kind: "note", text: "" },
                { kind: "note", text: "approved — device: mbp-16" },
              ]}
            />
            <Transcript
              caption="Pulling a skill into the current project"
              lines={[
                { kind: "command", text: "skl sync" },
                { kind: "output", text: "↓ 3 skills   ↑ 1 skill   0 conflicts" },
                { kind: "note", text: "" },
                { kind: "command", text: "skl use writing-tests" },
                { kind: "output", text: "→ .agents/skills/writing-tests" },
              ]}
            />
          </div>
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
