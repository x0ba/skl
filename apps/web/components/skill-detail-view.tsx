"use client";

import { useCallback, useMemo } from "react";
import { AuthGate } from "@/components/auth-gate";
import { ActionButton, ActionLink } from "@/components/ui/action-link";
import { Banner } from "@/components/ui/banner";
import { Meta } from "@/components/ui/field";
import { Lane, LaneEnd, LaneHead, LaneRow } from "@/components/ui/lane";
import { PageHeader } from "@/components/ui/page-header";
import { Label } from "@/components/ui/text";
import { getBlobText, getSkill } from "@/lib/api";
import type { SkillDetailResponse } from "@/lib/contracts";
import { exactTime, pluralize, relativeTime, shortHash, splitPath } from "@/lib/format";
import { useResource } from "@/lib/use-resource";

const COLS = "minmax(0,1fr) 8rem";
const SKILL_FILE = "SKILL.md";

type SkillDetail = SkillDetailResponse & {
  skillFile: { path: string; content: string } | null;
};

export function SkillDetailView({ name }: { name: string }) {
  const fetcher = useCallback(async (token: string): Promise<SkillDetail> => {
    const skill = await getSkill(token, name);
    const hash = skill.files[SKILL_FILE];
    if (!hash) return { ...skill, skillFile: null };
    return {
      ...skill,
      skillFile: { path: SKILL_FILE, content: await getBlobText(token, hash) },
    };
  }, [name]);
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
            <Label className="mb-4">{data.skillFile?.path ?? SKILL_FILE}</Label>
            {data.skillFile ? (
              <figure className="min-w-0 border border-border bg-secondary">
                <pre className="max-h-[32rem] min-w-0 overflow-y-auto overflow-x-hidden px-4 py-3.5 font-mono text-[13px] leading-[1.7] whitespace-pre-wrap break-words text-foreground">
                  <code className="whitespace-pre-wrap break-words">
                    {data.skillFile.content || "\u00A0"}
                  </code>
                </pre>
              </figure>
            ) : (
              <p className="border-t border-border py-10 font-mono text-[13px] text-faint">
                This tree has no {SKILL_FILE}.
              </p>
            )}
          </section>

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
        </div>
      ) : null}
    </>
  );
}
