import type { Metadata } from "next";
import { DeviceApproveForm } from "@/components/device-approve-form";
import { firstSearchParam } from "@/lib/auth-redirect";

export const metadata: Metadata = {
  title: "Approve a device",
};

export default async function DevicePage({ searchParams }: PageProps<"/device">) {
  const params = await searchParams;
  return <DeviceApproveForm initialUserCode={firstSearchParam(params.user_code)} />;
}
