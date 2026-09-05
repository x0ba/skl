import Link from "next/link";
import { CopyCommand } from "@/components/ui/copy-command";
import { Label } from "@/components/ui/text";
import { Transcript } from "@/components/ui/transcript";

const ROOTS = [
  { tool: "Claude", path: "~/.claude/skills" },
  { tool: "Cursor", path: "~/.cursor/skills" },
  { tool: "Codex", path: "~/.codex/skills" },
];

const STEPS = [
  {
    n: "01",
    title: "Authorize a machine",
    body: "The CLI prints a short code. You approve it in the browser once, and that machine holds a long-lived device token.",
    command: "skl login",
  },
  {
    n: "02",
    title: "Sync by content hash",
    body: "Every file is addressed by its SHA-256, and every skill by a hash of its tree. Only the blobs you are actually missing move over the wire.",
    command: "skl sync",
  },
  {
    n: "03",
    title: "Land in every agent's directory",
    body: "One store, written out to each tool's skills root. Edit a skill in Cursor, and Claude picks up the same version on the next sync.",
    command: "skl use writing-tests",
  },
];

export default function LandingPage() {
  return (
    <>
      <section className="mx-auto w-full max-w-content px-6 pb-20 pt-16 sm:pt-24">
        <div className="grid gap-14 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] lg:gap-16">
          <div>
            <Label className="mb-6">Personal agent skill sync</Label>
            <h1 className="max-w-xl text-balance font-sans text-display font-bold text-foreground">
              Your skills, on every machine.
            </h1>
            <p className="mt-6 max-w-md text-[17px] leading-relaxed text-muted-foreground">
              You wrote those skills once. skl keeps them identical across
              Claude, Cursor, and Codex — on your laptop, your desktop, and the
              box you only ssh into.
            </p>
            <div className="mt-9 max-w-md">
              <CopyCommand command="cargo install skl" />
            </div>
            <p className="mt-4 font-mono text-[12px] text-faint">
              Rust CLI · content-addressed · your own account
            </p>
          </div>

          <div className="space-y-6 lg:pt-14">
            <Transcript
              caption="First run on a new machine"
              lines={[
                { kind: "command", text: "skl login" },
                { kind: "output", text: "code: BQDF-7T2M" },
                { kind: "output", text: "open https://skl.sh/device" },
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
                { kind: "output", text: "→ ~/.claude/skills/writing-tests" },
                { kind: "output", text: "→ ~/.cursor/skills/writing-tests" },
              ]}
            />
          </div>
        </div>
      </section>

      <section className="border-t border-border">
        <div className="mx-auto w-full max-w-content px-6 py-16">
          <Label className="mb-8">What it touches</Label>
          <dl className="grid gap-px border-t border-border sm:grid-cols-3">
            {ROOTS.map((root) => (
              <div key={root.tool} className="border-b border-border py-6 sm:pr-8">
                <dt className="font-sans text-[17px] font-semibold text-foreground">
                  {root.tool}
                </dt>
                <dd className="mt-2 font-mono text-[13px] text-muted-foreground">
                  {root.path}
                </dd>
              </div>
            ))}
          </dl>
          <p className="mt-6 max-w-lg text-[14px] leading-relaxed text-muted-foreground">
            One canonical store, three directories. skl writes each skill where
            the agent already looks for it, so nothing in your tooling has to
            change.
          </p>
        </div>
      </section>

      <section className="border-t border-border">
        <div className="mx-auto w-full max-w-content px-6 py-16">
          <Label className="mb-8">How it works</Label>
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
        <div className="mx-auto w-full max-w-content px-6 py-20">
          <h2 className="max-w-lg text-balance font-sans text-[31px] font-bold tracking-[-0.035em] text-foreground">
            Stop copy-pasting skill folders between machines.
          </h2>
          <p className="mt-4 max-w-md text-[15px] leading-relaxed text-muted-foreground">
            Install the CLI, approve one device, and run{" "}
            <code className="font-mono text-foreground">skl sync</code>.
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
