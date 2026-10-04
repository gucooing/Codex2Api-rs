"use client";
import { useCallback, useEffect, useState } from "react";
import { request } from "./api";

export function useResource<T>(path: string, version: number) {
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<{
    path: string;
    version: number;
    revision: number;
    data?: T;
    error?: string;
  }>();
  const reload = useCallback(() => setRevision((v) => v + 1), []);
  useEffect(() => {
    const controller = new AbortController();
    request<T>(path, { signal: controller.signal })
      .then((data) => {
        if (!controller.signal.aborted) setResult({ path, version, revision, data });
      })
      .catch((reason) => {
        if (!controller.signal.aborted)
          setResult((old) => ({
            path,
            version,
            revision,
            data: old?.path === path ? old.data : undefined,
            error: reason instanceof Error ? reason.message : "加载失败",
          }));
      });
    return () => controller.abort();
  }, [path, version, revision]);
  const current =
    result?.path === path && result.version === version && result.revision === revision;
  return {
    data: result?.path === path ? result.data : undefined,
    ready: current && !!result.data && !result.error,
    error: current ? result.error : undefined,
    reload,
  };
}
