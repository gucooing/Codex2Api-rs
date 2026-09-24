"use client";
import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { useSearchParams } from "next/navigation";
import { request } from "./http";

export const ResourceRefreshContext = createContext(0);

export function useResource<T>(path: string | null, delayMs = 0) {
  const refreshVersion = useContext(ResourceRefreshContext);
  const [result, setResult] = useState<{
    path: string;
    revision: number;
    refreshVersion: number;
    data?: T;
    error?: string;
  }>();
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    if (!path) return;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      request<T>(path, { signal: controller.signal })
        .then((value) => {
          if (!controller.signal.aborted) {
            setResult({ path, revision, refreshVersion, data: value });
          }
        })
        .catch((e) => {
          if (!controller.signal.aborted) {
            setResult((previous) => ({
              path,
              revision,
              refreshVersion,
              data: previous?.path === path ? previous.data : undefined,
              error: e instanceof Error ? e.message : "加载失败",
            }));
          }
        });
    }, delayMs);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [path, revision, delayMs, refreshVersion]);
  const current =
    result?.path === path &&
    result?.revision === revision &&
    result?.refreshVersion === refreshVersion;
  const data = result?.path === path ? result.data : undefined;
  return {
    data,
    error: current ? (result.error ?? "") : "",
    loading: Boolean(path) && !current && data === undefined,
    refreshing: Boolean(path) && !current,
    ready: current && result.data != null && !result.error,
    reload,
  };
}
export function useQueryId() {
  return useSearchParams().get("id") ?? "";
}
