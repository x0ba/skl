import type { Metadata } from "next";
import { DeviceApproveForm } from "@/components/device-approve-form";

export const metadata: Metadata = {
  title: "Approve a device",
};

function first(value: string | string[] | undefined): string {
  if (Array.isArray(value)) {
    return value[0] ?? "";
  }
  return value ?? "";
}

export default async function DevicePage({ searchParams }: PageProps<"/device">) {
  const params = await searchParams;
  return <DeviceApproveForm initialUserCode={first(params.user_code)} />;
}
