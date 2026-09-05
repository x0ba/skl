"use client";

import { useCallback, useMemo } from "react";
import { AuthGate } from "@/components/auth-gate";
import { ActionButton, ActionLink } from "@/components/ui/action-link";
import { Banner } from "@/components/ui/banner";
import { CopyCommand } from "@/components/ui/copy-command";
import { Meta } from "@/components/ui/field";
import { Lane, LaneEnd, LaneHead, LaneRow } from "@/components/ui/lane";
import { PageHeader } from "@/components/ui/page-header";
import { Label } from "@/components/ui/text";
import { getSkill } from "@/lib/api";
import { exactTime, pluralize, relativeTime, shortHash, splitPath } from "@/lib/format";
import { useResource } from "@/lib/use-resource";

const COLS = "minmax(0,1fr) 8rem";

/** The agent skill roots the CLI writes into. Mirrors the CLI's target list. */
const TARGETS = [
  { tool: "Claude", root: "~/.claude/skills" },
  { tool: "Cursor", root: "~/.cursor/skills" },
  { tool: "Codex", root: "~/.codex/skills" },
];

export function SkillDetailView({ name }: { name: string }) {
  const fetcher = useCallback((token: string) => getSkill(token, name), [name]);
  const { data, error, loading, refreshing, unauthenticated, refresh } =
    useResource(fetcher);

  // Root files before nested ones, so SKILL.md — the entry point an agent
  // actually reads — leads instead of collating under `references/`.
  const files = useMemo(() => {
    if (!data) return [];
    return Object.entries(data.files).sort(([a], [b]) => {
      const depth = a.split("/").length - b.split("/").length;
      return depth !== 0 ? depth : a.localeCompare(b);
    });
  }, [data]);

  return (
    <>
      <PageHeader
        eyebrow="Skill"
        title={name}
        action={
          <div className="flex items-center gap-6">
            <ActionLink href="/skills">All skills</ActionLink>
            <ActionButton onClick={refresh} disabled={refreshing}>
              {refreshing ? "Refreshing…" : "Refresh"}
            </ActionButton>
          </div>
        }
      />

      {error ? (
        <Banner tone="danger" title="Could not load this skill" className="mb-8">
          {error}
        </Banner>
      ) : null}

      {unauthenticated ? (
        <AuthGate />
      ) : loading ? (
        <p className="border-t border-border py-14 font-mono text-[13px] text-faint">
          Loading…
        </p>
      ) : data ? (
        <div className="space-y-14">
          <dl className="grid gap-x-8 gap-y-6 border-t border-border pt-6 sm:grid-cols-3">
            <Meta label="Tree hash">
              <span className="break-all" title={data.tree_hash}>
                {shortHash(data.tree_hash)}
              </span>
            </Meta>
            <Meta label="Files">{pluralize(files.length, "file")}</Meta>
            <Meta label="Updated">
              <span title={exactTime(data.updated_at)}>
                {relativeTime(data.updated_at)}
              </span>
            </Meta>
          </dl>

          <section>
            <Label className="mb-4">Files</Label>
            {files.length === 0 ? (
              <p className="border-t border-border py-10 font-mono text-[13px] text-faint">
                This tree has no files.
              </p>
            ) : (
              <Lane cols={COLS}>
                <LaneHead>
                  <div>Path</div>
                  <LaneEnd>Blob</LaneEnd>
                </LaneHead>
                {files.map(([path, hash]) => {
                  const { dir, file } = splitPath(path);
                  return (
                    <LaneRow key={path}>
                      <div className="min-w-0 truncate font-mono text-[13px]">
                        {dir ? <span className="text-faint">{dir}</span> : null}
                        <span className="text-foreground">{file}</span>
                      </div>
                      <LaneEnd
                        className="font-mono text-[12px] text-faint"
                        title={hash}
                      >
                        {shortHash(hash)}
                      </LaneEnd>
                    </LaneRow>
                  );
                })}
              </Lane>
            )}
          </section>

          <section>
            <Label className="mb-4">Targets</Label>
            <Lane cols="7rem minmax(0,1fr)">
              <LaneHead>
                <div>Agent</div>
                <div>Path on disk</div>
              </LaneHead>
              {TARGETS.map((target) => (
                <LaneRow key={target.tool}>
                  <div className="text-[14px] text-foreground">{target.tool}</div>
                  <div className="truncate font-mono text-[13px] text-muted-foreground">
                    {`${target.root}/${data.name}`}
                  </div>
                </LaneRow>
              ))}
            </Lane>
            <p className="mt-4 max-w-prose text-[13px] leading-relaxed text-muted-foreground">
              Where this skill lands once you pull it down.
            </p>
            <CopyCommand
              command={`skl use ${data.name}`}
              className="mt-4 max-w-md"
            />
          </section>
        </div>
      ) : null}
    </>
  );
}
