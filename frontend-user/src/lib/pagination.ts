"use client";
import { useState, type InputHTMLAttributes } from "react";
import { toastError } from "./actions";
export function usePagination<T>(rows: T[], ready: boolean) {
  const [size, setSize] = useState(20);
  const [position, setPosition] = useState(1);
  const [draft, setDraft] = useState<string>();
  const pages = Math.max(1, Math.ceil(rows.length / size));
  const page = Math.min(position, pages);
  const commit = (value: string) => {
    const next = Number(value);
    setDraft(undefined);
    if (!/^\d+$/.test(value.trim()) || !Number.isSafeInteger(next) || next < 1 || next > pages) {
      toastError(`请输入 1 到 ${pages} 之间的页码`);
      return;
    }
    setPosition(next);
  };
  return {
    rows: rows.slice((page - 1) * size, page * size),
    total: ready ? rows.length : undefined,
    pages: ready ? pages : undefined,
    size: {
      value: String(size),
      disabled: !ready,
      onValueChange: (value: string) => {
        setSize(Number(value));
        setPosition(1);
        setDraft(undefined);
      },
    },
    first: { disabled: !ready || page <= 1, onClick: () => setPosition(1) },
    previous: { disabled: !ready || page <= 1, onClick: () => setPosition(page - 1) },
    next: { disabled: !ready || page >= pages, onClick: () => setPosition(page + 1) },
    last: { disabled: !ready || page >= pages, onClick: () => setPosition(pages) },
    input: {
      value: draft ?? String(page),
      disabled: !ready,
      onChange: (e: React.ChangeEvent<HTMLInputElement>) => setDraft(e.target.value),
      onBlur: (e: React.FocusEvent<HTMLInputElement>) => commit(e.target.value),
      onKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commit(e.currentTarget.value);
        }
      },
    },
  };
}

export function pageCount(total: number, pageSize = 20) {
  return Math.max(1, Math.ceil(total / pageSize));
}

/** Shared navigation state only; pages render the official controls directly. */
export function usePageControls(
  page: number,
  total: number | undefined,
  onPage: (page: number) => void,
  pageSize = 20,
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
