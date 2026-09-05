"use client";

import Link from "next/link";
import { useMemo } from "react";
import { AuthGate } from "@/components/auth-gate";
import { ActionButton } from "@/components/ui/action-link";
import { Banner } from "@/components/ui/banner";
import { CopyCommand } from "@/components/ui/copy-command";
import { EmptyState } from "@/components/ui/empty-state";
import { Lane, LaneEnd, LaneHead, LaneRow } from "@/components/ui/lane";
import { PageHeader } from "@/components/ui/page-header";
import { Label } from "@/components/ui/text";
import { Transcript } from "@/components/ui/transcript";
import { listSkills } from "@/lib/api";
import { exactTime, relativeTime, shortHash } from "@/lib/format";
import { useResource } from "@/lib/use-resource";

const COLS = "minmax(0,1fr) 8rem 6rem";

export function ActivityView() {
  const { data, error, loading, refreshing, unauthenticated, refresh } =
    useResource(listSkills);

  // Derived from each skill's `updated_at`. The API has no event log, so this
  // is a most-recently-changed ordering, not a full history.
  const recent = useMemo(() => {
    const skills = data?.skills ?? [];
    return [...skills].sort(
      (a, b) => Date.parse(b.updated_at) - Date.parse(a.updated_at),
    );
  }, [data]);

  return (
    <>
      <PageHeader
        eyebrow="Sync"
        title="Activity"
        description="Skills ordered by when they last changed on the server."
        action={
          <ActionButton onClick={refresh} disabled={refreshing}>
            {refreshing ? "Refreshing…" : "Refresh"}
          </ActionButton>
        }
      />

      {error ? (
        <Banner tone="danger" title="Could not load activity" className="mb-8">
          {error}
        </Banner>
      ) : null}

      {unauthenticated ? (
        <AuthGate />
      ) : loading ? (
        <p className="border-t border-border py-14 font-mono text-[13px] text-faint">
          Loading…
        </p>
      ) : recent.length === 0 ? (
        <EmptyState
          title="Nothing has synced yet"
          action={<CopyCommand command="skl sync" className="max-w-md" />}
        >
          Once a machine pushes skills to this account, its most recent changes
          show up here.
        </EmptyState>
      ) : (
        <div className="space-y-14">
          <Lane cols={COLS}>
            <LaneHead>
              <div>Skill</div>
              <div>Tree</div>
              <LaneEnd>Changed</LaneEnd>
            </LaneHead>
            {recent.map((skill) => (
              <LaneRow key={skill.name}>
                <div className="min-w-0">
                  <Link
                    href={`/skills/${encodeURIComponent(skill.name)}`}
                    className="block truncate font-mono text-[13px] text-foreground underline decoration-transparent underline-offset-2 hover:decoration-current"
                  >
                    {skill.name}
                  </Link>
                </div>
                <div className="truncate font-mono text-[12px] text-faint">
                  {shortHash(skill.tree_hash)}
                </div>
                <LaneEnd
                  className="font-mono text-[12px] text-faint"
                  title={exactTime(skill.updated_at)}
                >
                  {relativeTime(skill.updated_at)}
                </LaneEnd>
              </LaneRow>
            ))}
          </Lane>

          <section>
            <Label className="mb-4">Reading a sync</Label>
            <p className="mb-4 max-w-prose text-[13px] leading-relaxed text-muted-foreground">
              Sync is driven entirely by the CLI, and the server keeps no event
              log — so per-device history and conflict records live in the
              output of the command itself.
            </p>
            <Transcript
              lines={[
                { kind: "command", text: "skl sync" },
                { kind: "output", text: "↑ 2 blobs   ↓ 5 blobs" },
                { kind: "output", text: "↓ writing-tests   a1c4f0e2…" },
                { kind: "output", text: "↑ code-review     7b2e9d14…" },
                { kind: "note", text: "" },
                { kind: "note", text: "1 conflict: refactoring" },
                { kind: "note", text: "resolve with: skl sync --force-local" },
              ]}
            />
          </section>
        </div>
      )}
    </>
  );
}
