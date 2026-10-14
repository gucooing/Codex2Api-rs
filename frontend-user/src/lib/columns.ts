"use client";

import { useIsMobile } from "@/hooks/use-mobile";
import { usePreference } from "./preferences";

/** State only: each page renders the official column menu and its own table fields. */
export function useColumnVisibility(
  key: string,
  labels: readonly string[],
  compact: readonly string[],
) {
  const mobile = useIsMobile();
  const desktopDefaults = Object.fromEntries(labels.map((label) => [label, true]));
  const mobileDefaults = Object.fromEntries(
    labels.map((label) => [label, compact.includes(label)]),
  );
  const valid = (value: Record<string, boolean>) => labels.some((label) => value[label]);
  const [desktop, setDesktop] = usePreference(`columns:${key}:desktop`, desktopDefaults, valid);
  const [phone, setPhone] = usePreference(`columns:${key}:mobile`, mobileDefaults, valid);
  const selected = mobile ? phone : desktop;
  const update = mobile ? setPhone : setDesktop;
  const defaults = mobile ? mobileDefaults : desktopDefaults;
  const visible = labels.filter((label) => selected[label] ?? defaults[label]);
  return {
    labels,
    mobile,
    count: visible.length,
    isVisible: (label: string) => visible.includes(label),
    setVisible: (label: string, checked: boolean) => {
      if (!labels.includes(label) || (!checked && visible.length === 1 && visible.includes(label)))
        return;
      update((current) => ({ ...current, [label]: checked }));
    },
    reset: () => update(defaults),
    showAll: () => update(desktopDefaults),
  };
}
