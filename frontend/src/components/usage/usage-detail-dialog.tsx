"use client";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import { type UsageRecord } from "@/lib/api";
import { date, money } from "@/lib/format";
import { usageFormula } from "@/lib/usage-calculation";
import {
  cacheRate,
  duration,
  failedUsage,
  imageUsageLabel,
  isTextUsage,
  requestSpeed,
  tokenCount,
  usageFailure,
  usageResult,
  usageStatus,
  usageType,
  usageSourceDetail,
  usageConsumption,
} from "@/lib/usage-display";
import type { Dispatch, SetStateAction } from "react";

type Props = {
  selected: UsageRecord | undefined;
  setSelected: Dispatch<SetStateAction<UsageRecord | undefined>>;
};

export function UsageDetailDialog({ selected, setSelected }: Props) {
  return (
    <Dialog
      open={Boolean(selected)}
      onOpenChange={(open) => {
        if (!open) setSelected(undefined);
      }}
    >
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>请求详情</DialogTitle>
          <DialogDescription className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
            <span>{selected ? date(selected.requested_at_ms) : ""}</span>
            <span className="min-w-0 break-all text-xs">
              上游 ID：
              <span className="select-all font-mono">{selected?.upstream_request_id ?? "-"}</span>
            </span>
          </DialogDescription>
        </DialogHeader>
        <FieldGroup className="grid gap-3 sm:grid-cols-2">
          {[
            ["消费账户", selected?.consumer_name],
            [
              "供应账户",
              selected
                ? `${selected.account_name}${selected.account_provider_id ? ` · ${selected.account_provider_id}` : ""}`
                : null,
            ],
            ["计费倍率", selected ? `${selected.multiplier_micros / 1e6}×` : null],
            ["消费", selected ? money(usageConsumption(selected)) : null],
            ["请求模型", selected?.model],
            ["实际模型", selected?.actual_model],
            ["HTTP 状态码", selected?.http_status],
            ...(selected && failedUsage(selected)
              ? [
                  ["错误码", selected.error_code || "-"],
                  ["用户看到的错误", selected.public_error_code || "-"],
                ]
              : []),
            [
              "请求状态",
              selected
                ? usageResult(selected) +
                  (selected.status === "client_stopped" ? "（客户端停止）" : "")
                : "-",
            ],
            ["入站接口", selected?.endpoint],
            ["上游接口", selected?.upstream_endpoint ?? "—"],
            ["IP", selected?.client_ip ?? "—"],
            ["User-Agent", selected?.user_agent],
            ["来源", usageSourceDetail(selected)],
            ["类型", usageType(selected?.stream, selected?.transport)],
            ["首 Token 延迟", duration(selected?.first_token_ms)],
            ["总耗时", duration(selected?.total_ms)],
          ].map(([label, value]) => (
            <Field
              key={String(label)}
              className={label === "User-Agent" ? "sm:col-span-2" : undefined}
            >
              <FieldTitle>{label}</FieldTitle>
              <FieldDescription
                className={`break-words${label === "请求状态" && selected ? ` ${usageStatus(selected.status).className}` : ""}`}
              >
                {value ?? "-"}
              </FieldDescription>
            </Field>
          ))}
        </FieldGroup>
        {selected && failedUsage(selected) && (
          <FieldGroup className="gap-3 border-t pt-3">
            <Field>
              <FieldTitle>失败原因</FieldTitle>
              <FieldDescription className="whitespace-pre-wrap break-words">
                {usageFailure(selected)}
              </FieldDescription>
            </Field>
            {selected.public_error_message && (
              <Field>
                <FieldTitle>用户看到的原因</FieldTitle>
                <FieldDescription className="whitespace-pre-wrap break-words">
                  {selected.public_error_message}
                </FieldDescription>
              </Field>
            )}
          </FieldGroup>
        )}
        {selected && selected.attempts.length > 0 && (
          <FieldGroup className="gap-3 border-t pt-3">
            {selected.attempts.map((attempt) => (
              <Field key={attempt.attempt}>
                <FieldTitle>
                  第 {attempt.attempt} 次尝试 ·{" "}
                  {attempt.recovery === "retry" ? "同一供应账户重试" : "切换供应账户"}
                </FieldTitle>
                <FieldDescription className="whitespace-pre-wrap break-words">
                  {`${attempt.account_name} · ${attempt.account_provider_id} · HTTP ${attempt.http_status ?? "-"} · ${attempt.error_code ?? "-"} · ${duration(attempt.total_ms)}`}
                  {attempt.error_message ? `\n${attempt.error_message}` : ""}
                </FieldDescription>
              </Field>
            ))}
          </FieldGroup>
        )}
        {selected && (
          <FieldGroup className="grid gap-3 border-t pt-3 sm:grid-cols-3">
            {[
              ...(selected.image_count !== null
                ? [["图片用量", imageUsageLabel(selected)] as const]
                : []),

              ["输入", selected.input_tokens],
              ["输出", selected.output_tokens],
              ["思考（输出内）", selected.reasoning_tokens],
              ["缓存读取", selected.cached_tokens],
              ["缓存写入", selected.cache_write_tokens],
              ["缓存率", cacheRate(selected)],
              ["费用", selected.cost_nano_usd == null ? "-" : money(selected.cost_nano_usd / 1e9)],
              ...(isTextUsage(selected) ? [["请求速度", requestSpeed(selected)] as const] : []),
            ].map(([label, value]) => (
              <Field key={String(label)}>
                <FieldTitle>{label}</FieldTitle>
                <FieldDescription
                  title={typeof value === "number" ? value.toLocaleString("en-US") : undefined}
                >
                  {typeof value === "number" ? tokenCount(value) : (value ?? "-")}
                </FieldDescription>
              </Field>
            ))}
            <Field className="sm:col-span-3">
              <FieldTitle>成本</FieldTitle>
              <FieldDescription className="break-words">
                {usageFormula(selected.cost_calculation)}
              </FieldDescription>
            </Field>
          </FieldGroup>
        )}
      </DialogContent>
    </Dialog>
  );
}
