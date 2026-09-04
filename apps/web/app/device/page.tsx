import { DeviceApproveForm } from "@/components/device-approve-form";

function first(value: string | string[] | undefined): string {
  if (Array.isArray(value)) {
    return value[0] ?? "";
  }
  return value ?? "";
}

export default async function DevicePage({
  searchParams,
}: {
  searchParams: Promise<{ user_code?: string | string[] }>;
}) {
  const params = await searchParams;
  return <DeviceApproveForm initialUserCode={first(params.user_code)} />;
}
