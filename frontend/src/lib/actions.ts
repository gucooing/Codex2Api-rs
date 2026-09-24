"use client";

import { useEffect, useRef, useSyncExternalStore, type FormEvent } from "react";
import { toast } from "sonner";

type Action = {
  key: string;
  execute: () => Promise<unknown>;
  confirm?: string;
  danger?: boolean;
  success?: string;
};
type State = { running: ReadonlySet<string>; pending?: Action };
const idle: State = { running: new Set() };
let state = idle;
let focusTarget: HTMLElement | null = null;
const listeners = new Set<() => void>();
function publish(next: State) {
  state = next;
  listeners.forEach((listener) => listener());
}
function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
export function toastError(error: unknown) {
  const message =
    error instanceof TypeError && /fetch|network|load failed/i.test(error.message)
      ? "无法连接服务，请检查网络后重试。"
      : error instanceof Error
        ? error.message
        : typeof error === "string"
          ? error
          : "操作失败，请重试。";
  toast.error(message, { id: `error-${message}`, duration: 5000 });
}
export function useErrorToast(message: string | undefined | null) {
  useEffect(() => {
    if (message) toastError(message);
  }, [message]);
}
export function useDialogFocus() {
  const trigger = useRef<HTMLElement | null>(null);
  return {
    onOpenAutoFocus: () => {
      trigger.current = document.activeElement as HTMLElement | null;
    },
    onCloseAutoFocus: (event: Event) => {
      event.preventDefault();
      if (trigger.current?.isConnected) trigger.current.focus();
    },
  };
}
export function validateForm(form: HTMLFormElement): boolean {
  const requiredSelect = form.querySelector<HTMLElement>(
    '[data-required="true"][data-empty="true"]:not(:disabled)',
  );
  const control = Array.from(
    form.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(
      "input, select, textarea",
    ),
  ).find((item) => item.willValidate && !item.validity.valid);
  const invalid = control ?? requiredSelect;
  if (!invalid) return true;
  const label =
    invalid
      .closest('[data-slot="field"]')
      ?.querySelector('[data-slot="field-label"]')
      ?.textContent?.replace(/\*/g, "")
      .trim() ||
    invalid.getAttribute("aria-label") ||
    "此字段";
  let message = `请检查${label}的格式`;
  if (!control || control.validity.valueMissing)
    message = `${invalid instanceof HTMLSelectElement || invalid === requiredSelect ? "请选择" : "请填写"}${label}`;
  else if (control.validity.typeMismatch) message = `请输入有效的${label}`;
  else if (control instanceof HTMLInputElement && control.validity.rangeUnderflow)
    message = `${label}不能小于 ${control.min}`;
  else if (control instanceof HTMLInputElement && control.validity.rangeOverflow)
    message = `${label}不能大于 ${control.max}`;
  toastError(message);
  return false;
}
async function execute(action: Action) {
  if (state.running.has(action.key)) return;
  publish({ ...state, running: new Set([...state.running, action.key]) });
  try {
    await action.execute();
    if (action.success !== "") toast.success(action.success ?? "操作已完成");
    if (state.pending === action) publish({ ...state, pending: undefined });
  } catch (error) {
    toastError(error);
  } finally {
    const running = new Set(state.running);
    running.delete(action.key);
    publish({ ...state, running });
  }
}
function run(
  key: string,
  callback: () => Promise<unknown>,
  options: Omit<Partial<Action>, "key" | "execute"> = {},
) {
  if (state.running.has(key)) return;
  const action = { key, execute: callback, ...options };
  if (!action.confirm) return execute(action);
  const active = document.activeElement as HTMLElement | null;
  const menu = active?.closest('[role="menu"]');
  focusTarget = menu?.getAttribute("aria-labelledby")
    ? document.getElementById(menu.getAttribute("aria-labelledby")!)
    : active;
  publish({ ...state, pending: action });
}
export function dismissConfirmation() {
  if (state.pending && state.running.has(state.pending.key)) return;
  publish({ ...state, pending: undefined });
}
export function restoreConfirmationFocus() {
  if (focusTarget?.isConnected) focusTarget.focus();
}
function submit(
  event: FormEvent<HTMLFormElement>,
  key: string,
  callback: () => Promise<unknown>,
  success = "已保存",
) {
  event.preventDefault();
  if (validateForm(event.currentTarget)) void run(key, callback, { success });
}
export function useActions() {
  const current = useSyncExternalStore(
    subscribe,
    () => state,
    () => idle,
  );
  return {
    ...current,
    isBusy: (key: string) => current.running.has(key),
    run,
    submit,
    confirm: () => {
      if (state.pending) void execute(state.pending);
    },
    dismiss: dismissConfirmation,
  };
}
