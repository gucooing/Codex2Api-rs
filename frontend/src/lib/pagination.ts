"use client";

import { useState, type InputHTMLAttributes } from "react";
import { toastError } from "./actions";

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

export function useTablePagination<T>(rows: T[], scope: unknown = "", ready = true) {
  const [position, setPosition] = useState({ scope, page: 1 });
  const [pageSize, setPageSize] = useState(TABLE_PAGE_SIZE);
  const page =
    position.scope === scope ? Math.min(position.page, pageCount(rows.length, pageSize)) : 1;
  const controls = usePageControls(
    page,
    ready ? rows.length : undefined,
    (page) => setPosition({ scope, page }),
    pageSize,
    false,
    setPageSize,
  );
  return { ...controls, rows: rows.slice((page - 1) * pageSize, page * pageSize) };
}
