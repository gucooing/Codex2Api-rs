"use client";

import { useCallback, useMemo, useSyncExternalStore, type SetStateAction } from "react";

const prefix = "codex2api-user-ui-v1:";
const listeners = new Map<string, Set<() => void>>();
const snapshots = new Map<string, { raw: string | null; value: unknown }>();
const volatile = new Map<string, unknown>();

// Only known preference fields and matching primitive types are restored.
// Never use this store for credentials, business forms, row data or write selections.
function restore(value: unknown, fallback: unknown): unknown {
  if (typeof fallback === "string")
    return typeof value === "string" && value.length <= 4096 ? value : fallback;
  if (typeof fallback === "number")
    return typeof value === "number" && Number.isFinite(value) ? value : fallback;
  if (typeof fallback === "boolean") return typeof value === "boolean" ? value : fallback;
  if (fallback && typeof fallback === "object" && !Array.isArray(fallback)) {
    const input =
      value && typeof value === "object" && !Array.isArray(value)
        ? (value as Record<string, unknown>)
        : {};
    return Object.fromEntries(
      Object.entries(fallback).map(([key, defaultValue]) => [
        key,
        restore(input[key], defaultValue),
      ]),
    );
  }
  return fallback;
}

function read<T>(key: string, fallback: T, valid?: (value: T) => boolean): T {
  if (volatile.has(key)) return volatile.get(key) as T;
  let raw: string | null;
  try {
    raw = window.localStorage.getItem(prefix + key);
  } catch {
    return fallback;
  }
  const previous = snapshots.get(key);
  if (previous?.raw === raw) return previous.value as T;
  let value = fallback;
  if (raw) {
    try {
      const candidate = restore(JSON.parse(raw), fallback) as T;
      if (!valid || valid(candidate)) value = candidate;
    } catch {
      /* Corrupt or outdated local preferences fall back to the page defaults. */
    }
  }
  snapshots.set(key, { raw, value });
  return value;
}

const subscribeHydration = () => () => {};
const clientSnapshot = () => true;
const serverSnapshot = () => false;

export function usePreference<T>(key: string, defaultValue: T, valid?: (value: T) => boolean) {
  const defaultsJson = JSON.stringify(defaultValue);
  const fallback = useMemo(() => JSON.parse(defaultsJson) as T, [defaultsJson]);
  const ready = useSyncExternalStore(subscribeHydration, clientSnapshot, serverSnapshot);
  const subscribe = useCallback(
    (listener: () => void) => {
      const set = listeners.get(key) ?? new Set<() => void>();
      listeners.set(key, set);
      set.add(listener);
      const storage = (event: StorageEvent) => {
        if (event.key === prefix + key || event.key === null) {
          snapshots.delete(key);
          volatile.delete(key);
          listener();
        }
      };
      window.addEventListener("storage", storage);
      return () => {
        set.delete(listener);
        window.removeEventListener("storage", storage);
      };
    },
    [key],
  );
  const value = useSyncExternalStore(
    subscribe,
    () => read(key, fallback, valid),
    () => fallback,
  );
  const update = useCallback(
    (action: SetStateAction<T>) => {
      const current = read(key, fallback, valid);
      const next = typeof action === "function" ? (action as (value: T) => T)(current) : action;
      const raw = JSON.stringify(next);
      try {
        window.localStorage.setItem(prefix + key, raw);
        volatile.delete(key);
      } catch {
        volatile.set(key, next);
      }
      snapshots.set(key, { raw, value: next });
      listeners.get(key)?.forEach((listener) => listener());
    },
    [key, fallback, valid],
  );
  return [value, update, ready] as const;
}
