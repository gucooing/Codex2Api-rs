"use client";
import { useState, useId } from "react";
import {
  request,
  type Consumer,
  type List,
  type Supplier,
  type SupplierTag,
  type RpmLimit,
} from "@/lib/api";
import { useResource } from "@/lib/hooks";
import { useActions, useErrorToast } from "@/lib/actions";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel, FieldDescription, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

export type RoutingResponse = {
  items: {
    virtual_account_id: string;
    provider_id: string;
    supplier_account_id: string | null;
    tag_id: string | null;
    revision: number;
  }[];
};

export function RoutingForm({
  account,
  data,
  disabled,
  onSaved,
}: {
  account?: Consumer;
  data: RoutingResponse;
  disabled: boolean;
  onSaved: () => void;
}) {
  const id = useId();
  const actions = useActions();
  const tags = useResource<List<SupplierTag>>("/supplier-tags");
  const suppliers = useResource<List<Supplier>>("/suppliers");
  const rpm = useResource<RpmLimit>(account ? `/consumers/${account.id}/rate-limit` : null);
  const route = data.items.find((r) => r.provider_id === account?.provider_id);
  const [draft, setDraft] = useState<{ tag: string; supplier: string }>();
  const current = draft ?? { tag: route?.tag_id ?? "", supplier: route?.supplier_account_id ?? "" };
  const [rpmDraft, setRpm] = useState<string>();
  const rpmValue = rpmDraft ?? (rpm.data?.rpm == null ? "" : String(rpm.data.rpm));
  const available = (suppliers.data?.items ?? []).filter(
    (s) => s.provider_id === account?.provider_id && s.tag_ids?.includes(current.tag),
  );
  const assigned = suppliers.data?.items.find((supplier) => supplier.id === current.supplier);
  useErrorToast(tags.error);
  useErrorToast(suppliers.error);
  useErrorToast(rpm.error);
  return (
    <div className="space-y-4">
      <form
        noValidate
        className="space-y-3"
        onSubmit={(event) =>
          actions.submit(event, "pool-route", async () => {
            if (disabled || !account || !tags.ready || !suppliers.ready)
              throw new Error("请先加载号池和供应账户");
            await request(`/consumers/${account.id}/routing`, {
              method: "PUT",
              body: {
                tag_id: current.tag || null,
                supplier_id: current.supplier || null,
                revision: route?.revision ?? null,
              },
            });
            setDraft(undefined);
            onSaved();
            suppliers.reload();
          })
        }
      >
        <FieldSet
          disabled={disabled || !tags.ready || !suppliers.ready || actions.isBusy("pool-route")}
        >
          <Field>
            <FieldLabel htmlFor={`${id}-tag`}>标签号池</FieldLabel>
            <Select
              value={current.tag || "none"}
              onValueChange={(tag) => {
                if (
                  disabled ||
                  !tags.ready ||
                  !suppliers.ready ||
                  !tag ||
                  tag === (current.tag || "none")
                )
                  return;
                setDraft({ tag: tag === "none" ? "" : tag, supplier: "" });
              }}
            >
              <SelectTrigger id={`${id}-tag`}>
                <SelectValue placeholder="选择标签号池" />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="none">不绑定号池</SelectItem>
                {(tags.data?.items ?? [])
                  .filter((t) => t.provider_id === account?.provider_id)
                  .map((t) => (
                    <SelectItem key={t.id} value={t.id}>
                      {t.name}（{t.supplier_count} 个供应账户）
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor={`${id}-supplier`}>分配账户</FieldLabel>
            <Select
              value={disabled && !route && !draft ? "" : current.supplier || "unassigned"}
              disabled={!current.tag}
              onValueChange={(supplier) => {
                if (
                  disabled ||
                  !tags.ready ||
                  !suppliers.ready ||
                  !supplier ||
                  supplier === "unassigned" ||
                  supplier === current.supplier
                )
                  return;
                setDraft({ ...current, supplier });
              }}
            >
              <SelectTrigger id={`${id}-supplier`}>
                <SelectValue placeholder={disabled ? "加载中…" : "暂未分配"} />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="unassigned" disabled>
                  暂未分配
                </SelectItem>
                {current.supplier &&
                  !available.some((supplier) => supplier.id === current.supplier) && (
                    <SelectItem value={current.supplier} disabled>
                      {assigned?.display_name || assigned?.email || current.supplier}
                    </SelectItem>
                  )}
                {available.map((s) => (
                  <SelectItem
                    key={s.id}
                    value={s.id}
                    disabled={s.status !== "active" || !s.authorized}
                  >
                    {s.display_name || s.email || s.id}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Button type="submit">保存绑定</Button>
        </FieldSet>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => {
            onSaved();
            tags.reload();
            suppliers.reload();
            rpm.reload();
          }}
        >
          刷新
        </Button>
      </form>
      <form
        noValidate
        className="space-y-3"
        onSubmit={(event) =>
          actions.submit(event, "consumer-rpm", async () => {
            if (!account || !rpm.ready) throw new Error("请先加载 RPM 配置");
            const value = rpmValue.trim() === "" ? null : Number(rpmValue);
            if (value !== null && (!Number.isInteger(value) || value < 0 || value > 1_000_000))
              throw new Error("RPM 须为 0 到 1000000 的整数");
            await request(`/consumers/${account.id}/rate-limit`, {
              method: "PUT",
              body: { rpm: value },
            });
            setRpm(undefined);
            rpm.reload();
          })
        }
      >
        <FieldSet disabled={!rpm.ready || actions.isBusy("consumer-rpm")}>
          <Field>
            <FieldLabel htmlFor={`${id}-rpm`}>RPM 限制</FieldLabel>
            <Input
              id={`${id}-rpm`}
              type="number"
              min={0}
              step={1}
              value={rpmValue}
              placeholder="继承默认限制"
              onChange={(event) => setRpm(event.target.value)}
            />
            <FieldDescription>
              留空用默认值（{rpm.data?.default_rpm ?? "—"}），0 为无限。
            </FieldDescription>
          </Field>
          <Button type="submit">保存 RPM</Button>
        </FieldSet>
      </form>
    </div>
  );
}
