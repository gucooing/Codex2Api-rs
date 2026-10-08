"use client";

import { useState, type InputHTMLAttributes } from "react";
import { toastError } from "./actions";
import { usePathname } from "next/navigation";
import { usePreference, validPageSize } from "./preferences";
import { useResource } from "./hooks";
import { query } from "./api";

export const TABLE_PAGE_SIZE = 20;

export function pageCount(total: number, pageSize = TABLE_PAGE_SIZE) {
  return Math.max(1, Math.ceil(total / pageSize));
}

/** Shared navigation state only; pages render the official controls directly. */
export function usePageControls(
  page: number,
  total: number | undefined,
  onPage: (page: number) => void,
  pageSize = TABLE_PAGE_SIZE,
  disabled = false,
  onPageSize?: (size: number) => void,
) {
  const pages = total === undefined ? undefined : pageCount(total, pageSize);
  const [draft, setDraft] = useState<{ page: number; text: string }>();
  const commit = (text: string) => {
    if (pages === undefined || disabled) return;
    const next = Number(text);
    if (!/^\d+$/.test(text.trim()) || !Number.isSafeInteger(next) || next < 1 || next > pages) {
      toastError(`请输入 1 到 ${pages} 之间的页码`);
      setDraft(undefined);
      return;
    }
    setDraft(undefined);
    if (next !== page) onPage(next);
  };
  const input: InputHTMLAttributes<HTMLInputElement> = {
    "aria-label": "页码",
    inputMode: "numeric",
    value: draft?.page === page ? draft.text : String(page),
    disabled: disabled || pages === undefined,
    onChange: (event) => setDraft({ page, text: event.target.value }),
    onBlur: (event) => commit(event.target.value),
    onKeyDown: (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        commit(event.currentTarget.value);
      } else if (event.key === "Escape") {
        setDraft(undefined);
      }
    },
  };
  return {
    page,
    pages,
    total,
    input,
    size: {
      value: String(pageSize),
      disabled,
      onValueChange: (value: string) => {
        const size = Number(value);
        if ([10, 20, 30, 50].includes(size)) {
          setDraft(undefined);
          onPage(1);
          onPageSize?.(size);
        }
      },
    },
    first: { disabled: disabled || pages === undefined || page <= 1, onClick: () => onPage(1) },
    previous: {
      disabled: disabled || pages === undefined || page <= 1,
      onClick: () => onPage(page - 1),
    },
    next: {
      disabled: disabled || pages === undefined || page >= pages,
      onClick: () => onPage(page + 1),
    },
    last: {
      disabled: disabled || pages === undefined || page >= pages,
      onClick: () => {
        if (pages !== undefined) onPage(pages);
      },
    },
  };
}

export type ListPage<T> = { items: T[]; total: number; page: number; page_size: number };

export function useListResource<T, Extra extends object = Record<never, never>>(
  path: string | null,
  filters: Record<string, string | number | boolean | null | undefined> = {},
  ready = true,
) {
  const scope = `${path}:${JSON.stringify(filters)}`;
  const [position, setPosition] = useState({ scope, page: 1 });
  const pathname = usePathname();
  const [pageSize, setPageSize, preferencesReady] = usePreference<number>(
    `page-size:${pathname}`,
    TABLE_PAGE_SIZE,
    validPageSize,
  );
  const page = position.scope === scope ? position.page : 1;
  const params = query({ ...filters, page, page_size: pageSize });
  const resource = useResource<ListPage<T> & Extra>(
    path && ready && preferencesReady
      ? `${path}${path.includes("?") ? "&" + params.slice(1) : params}`
      : null,
  );
  const controls = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    (page) => setPosition({ scope, page }),
    pageSize,
    resource.refreshing,
    setPageSize,
  );
  return {
    ...resource,
    reload: (firstPage?: unknown) => {
      if (firstPage === 1) setPosition({ scope, page: firstPage });
      resource.reload();
    },
    pagination: { ...controls, rows: resource.data?.items ?? [] },
  };
}
