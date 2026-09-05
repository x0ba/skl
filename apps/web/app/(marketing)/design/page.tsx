import type { Metadata } from "next";
import { Banner } from "@/components/ui/banner";
import { CopyCommand } from "@/components/ui/copy-command";
import { EmptyState } from "@/components/ui/empty-state";
import { Field, Input, Meta } from "@/components/ui/field";
import { Lane, LaneEnd, LaneHead, LaneRow } from "@/components/ui/lane";
import { StatusDot, type Status } from "@/components/ui/status-dot";
import { Heading, Label, Mono } from "@/components/ui/text";
import { Transcript } from "@/components/ui/transcript";

export const metadata: Metadata = {
  title: "Design system",
};

const COLORS = [
  { token: "--background", value: "#ffffff", note: "Page ground" },
  { token: "--foreground", value: "#0a0a0a", note: "Body text" },
  { token: "--muted-foreground", value: "#5c5c5c", note: "Secondary prose" },
  { token: "--faint", value: "#787878", note: "Hashes, timestamps" },
  { token: "--primary", value: "#0b24fb", note: "Actionable. Sparingly." },
  { token: "--destructive", value: "#cc1100", note: "Revoke, delete" },
  { token: "--border", value: "#e4e4e4", note: "Structural rules" },
  { token: "--rule-soft", value: "#ededed", note: "Between repeated rows" },
  { token: "--secondary", value: "#fafafa", note: "Inset panels" },
];

const TYPE = [
  { name: "Display", className: "text-display font-bold", sample: "Aa" },
  {
    name: "Title",
    className: "text-[31px] font-bold tracking-[-0.035em]",
    sample: "Skills",
  },
  {
    name: "Heading",
    className: "text-[19px] font-semibold tracking-[-0.025em]",
    sample: "Files",
  },
  { name: "Body", className: "text-[15px] leading-relaxed", sample: "Body copy" },
  { name: "Data", className: "font-mono text-[13px]", sample: "a1c4f0e2d938" },
  {
    name: "Label",
    className: "font-mono text-[11px] font-medium tracking-label text-faint",
    sample: "TREE HASH",
  },
];

const STATUSES: Status[] = ["synced", "pending", "conflict", "revoked"];

function Section({
  n,
  title,
  children,
  note,
}: {
  n: string;
  title: string;
  note?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="border-t border-border py-14">
      <div className="mb-8 flex items-baseline gap-4">
        <Label>{n}</Label>
        <Heading>{title}</Heading>
      </div>
      {note ? (
        <p className="mb-8 max-w-prose text-[14px] leading-relaxed text-muted-foreground">
          {note}
        </p>
      ) : null}
      {children}
    </section>
  );
}

export default function DesignSystemPage() {
  return (
    <div className="mx-auto w-full max-w-content px-6 pb-24 pt-16">
      <header className="pb-4">
        <Label className="mb-4">Reference</Label>
        <h1 className="max-w-2xl text-balance font-sans text-[44px] font-bold leading-[1.05] tracking-[-0.04em] text-foreground">
          Hypertext
        </h1>
        <p className="mt-5 max-w-lg text-[16px] leading-relaxed text-muted-foreground">
          White ground, ink text, one hyperlink blue. Two rules hold it
          together: blue marks what is actionable, and nothing has a corner
          radius.
        </p>
      </header>

      <Section
        n="01"
        title="Color"
        note="Nine values. The accent appears at most twice per screen, so if a third blue thing shows up, one of them is not really an action."
      >
        <Lane cols="2.5rem minmax(0,1fr) 7rem minmax(0,1fr)">
          <LaneHead>
            <div />
            <div>Token</div>
            <div>Value</div>
            <div>Role</div>
          </LaneHead>
          {COLORS.map((color) => (
            <LaneRow key={color.token}>
              <div
                aria-hidden
                className="size-5 border border-border"
                style={{ background: color.value }}
              />
              <Mono className="truncate text-foreground">{color.token}</Mono>
              <Mono className="text-faint">{color.value}</Mono>
              <div className="text-[13px] text-muted-foreground">{color.note}</div>
            </LaneRow>
          ))}
        </Lane>
      </Section>

      <Section
        n="02"
        title="Type"
        note="Geist for prose and interface, Geist Mono for anything the machine produced — hashes, paths, counts, commands."
      >
        <Lane cols="7rem minmax(0,1fr)">
          <LaneHead>
            <div>Role</div>
            <div>Sample</div>
          </LaneHead>
          {TYPE.map((entry) => (
            <LaneRow key={entry.name}>
              <Mono className="text-faint">{entry.name}</Mono>
              <div className={entry.className}>{entry.sample}</div>
            </LaneRow>
          ))}
        </Lane>
      </Section>

      <Section
        n="03"
        title="Status"
        note="A 5px square plus a word. Status never depends on color alone."
      >
        <div className="flex flex-wrap gap-x-10 gap-y-4">
          {STATUSES.map((status) => (
            <StatusDot key={status} status={status} />
          ))}
        </div>
      </Section>

      <Section
        n="04"
        title="Lanes"
        note="This system's table: a mono all-caps header, hairline rules between rows, no vertical borders and no zebra striping."
      >
        <Lane cols="minmax(0,1fr) 8rem 6rem">
          <LaneHead>
            <div>Skill</div>
            <div>Tree</div>
            <LaneEnd>Updated</LaneEnd>
          </LaneHead>
          <LaneRow>
            <Mono className="text-foreground">writing-tests</Mono>
            <Mono className="text-[12px] text-faint">a1c4f0e2d938</Mono>
            <LaneEnd className="font-mono text-[12px] text-faint">4h ago</LaneEnd>
          </LaneRow>
          <LaneRow>
            <Mono className="text-foreground">code-review</Mono>
            <Mono className="text-[12px] text-faint">7b2e9d1406af</Mono>
            <LaneEnd className="font-mono text-[12px] text-faint">2d ago</LaneEnd>
          </LaneRow>
        </Lane>
      </Section>

      <Section n="05" title="Notices">
        <div className="space-y-4">
          <Banner title="Sync finished with 1 conflict">
            The remote copy of <code className="font-mono">refactoring</code> is
            newer than the local one.
          </Banner>
          <Banner tone="danger" title="Could not load devices">
            The API returned 401 missing_authorization.
          </Banner>
        </div>
      </Section>

      <Section
        n="06"
        title="Inputs"
        note="A baseline to type on rather than a box to fill in."
      >
        <div className="max-w-sm space-y-8">
          <Field
            label="Device name"
            htmlFor="ds-device"
            hint="Shown in the devices list."
          >
            <Input id="ds-device" placeholder="mbp-16" />
          </Field>
          <Field label="Bearer token" htmlFor="ds-token" error="Token is expired.">
            <Input id="ds-token" defaultValue="dev:user_123" />
          </Field>
          <div>
            <Label className="mb-3">Metadata</Label>
            <dl className="grid grid-cols-2 gap-6">
              <Meta label="Tree hash">a1c4f0e2d938</Meta>
              <Meta label="Files">7 files</Meta>
            </dl>
          </div>
        </div>
      </Section>

      <Section
        n="07"
        title="Commands"
        note="The CLI is the primary interface, so its output is quoted directly rather than paraphrased into prose."
      >
        <div className="max-w-lg space-y-6">
          <CopyCommand command="cargo install skl" />
          <Transcript
            caption="Approving a new machine"
            lines={[
              { kind: "command", text: "skl login" },
              { kind: "output", text: "code: BQDF-7T2M" },
              { kind: "note", text: "approved — device: mbp-16" },
            ]}
          />
        </div>
      </Section>

      <Section n="08" title="Empty states">
        <EmptyState
          title="No skills yet"
          action={<CopyCommand command="skl sync" className="max-w-md" />}
        >
          Nothing has been pushed to this account. Run a sync from a machine
          that already has skills on disk.
        </EmptyState>
      </Section>
    </div>
  );
}
