"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type Device } from "@/lib/api";
import { useListResource } from "@/lib/pagination";

export function useDevicesPage() {
  const devices = useListResource<Device>("/devices");
  const devicePage = devices.pagination;
  const actions = useActions();
  useErrorToast(devices.error);
  const handleClick = (d: Device) =>
    void actions.run(d.id, async () => {
      await request(`/devices/${encodeURIComponent(d.id)}/revoke`, {
        method: "POST",
      });
      devices.reload();
    });
  return { devices, devicePage, actions, handleClick } as const;
}
