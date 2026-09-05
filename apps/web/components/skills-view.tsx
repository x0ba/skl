"use client";

import Link from "next/link";
import { AuthGate } from "@/components/auth-gate";
import { ActionButton } from "@/components/ui/action-link";
import { Banner } from "@/components/ui/banner";
import { CopyCommand } from "@/components/ui/copy-command";
import { EmptyState } from "@/components/ui/empty-state";
import { Lane, LaneEnd, LaneHead, LaneLinkRow } from "@/components/ui/lane";
import { PageHeader } from "@/components/ui/page-header";
import { listSkills } from "@/lib/api";
import { exactTime, relativeTime, shortHash } from "@/lib/format";
import { useResource } from "@/lib/use-resource";

const COLS = "minmax(0,1fr) 8rem 6rem";

export function SkillsView() {
  const { data, error, loading, refreshing, unauthenticated, refresh } =
    useResource(listSkills);

  const skills = data?.skills ?? [];

  return (
    <>
      <PageHeader
        eyebrow="Library"
        title="Skills"
        description="Every skill in your account, newest change first."
        action={
          <ActionButton onClick={refresh} disabled={refreshing}>
            {refreshing ? "Refreshing…" : "Refresh"}
          </ActionButton>
        }
      />

      {error ? (
        <Banner tone="danger" title="Could not load skills" className="mb-8">
          {error}
        </Banner>
      ) : null}

      {unauthenticated ? (
        <AuthGate />
      ) : loading ? (
        <SkillsSkeleton />
      ) : skills.length === 0 ? (
        <EmptyState
          title="No skills yet"
          action={<CopyCommand command="skl sync" className="max-w-md" />}
        >
          Nothing has been pushed to this account. Run a sync from a machine
          that already has skills on disk.
        </EmptyState>
      ) : (
        <Lane cols={COLS}>
          <LaneHead>
            <div>Skill</div>
            <div>Tree</div>
            <LaneEnd>Updated</LaneEnd>
          </LaneHead>
          {skills.map((skill) => (
            <LaneLinkRow key={skill.name}>
              <div className="min-w-0">
                <Link
                  href={`/skills/${encodeURIComponent(skill.name)}`}
                  className="block truncate font-mono text-[13px] text-foreground underline decoration-transparent underline-offset-2 hover:decoration-current focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
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
            </LaneLinkRow>
          ))}
        </Lane>
      )}
    </>
  );
}

function SkillsSkeleton() {
  return (
    <Lane cols={COLS} aria-busy>
      <LaneHead>
        <div>Skill</div>
        <div>Tree</div>
        <LaneEnd>Updated</LaneEnd>
      </LaneHead>
      {[0, 1, 2, 3].map((row) => (
        <LaneLinkRow key={row}>
          <div className="h-3 w-40 bg-secondary" />
          <div className="h-3 w-20 bg-secondary" />
          <LaneEnd className="h-3 w-14 bg-secondary" />
        </LaneLinkRow>
      ))}
    </Lane>
  );
}
