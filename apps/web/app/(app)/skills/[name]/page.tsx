import type { Metadata } from "next";
import { SkillDetailView } from "@/components/skill-detail-view";

export async function generateMetadata({
  params,
}: PageProps<"/skills/[name]">): Promise<Metadata> {
  const { name } = await params;
  return { title: decodeURIComponent(name) };
}

export default async function SkillDetailPage({
  params,
}: PageProps<"/skills/[name]">) {
  const { name } = await params;
  return <SkillDetailView name={decodeURIComponent(name)} />;
}
