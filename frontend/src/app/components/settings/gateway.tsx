"use client";
import { GatewayClientRules } from "@/app/components/settings/gateway-client-rules";
import { GatewayLimits } from "@/app/components/settings/gateway-limits";
import { GatewayScheduling } from "@/app/components/settings/gateway-scheduling";
import { useGateway } from "@/app/data/settings";
import { Button } from "@/components/ui/button";
import { FieldSet } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";

export function Gateway() {
  const { resource, form, busy, disabled, handleSubmit } = useGateway();
  return (
    <form
      noValidate
      className="space-y-4"
      aria-busy={busy || resource.refreshing}
      onSubmit={handleSubmit}
    >
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={resource.reload}
          disabled={resource.refreshing || busy}
        >
          {resource.refreshing && <Spinner />}
          {resource.ready ? "刷新" : "重新加载"}
        </Button>
        <Button type="submit" size="sm" disabled={disabled}>
          {busy && <Spinner />}
          {busy ? "正在保存…" : "保存"}
        </Button>
      </div>
      <FieldSet className="gap-4">
        <GatewayLimits form={form} />
        <GatewayScheduling form={form} />
        <GatewayClientRules form={form} />
      </FieldSet>
    </form>
  );
}
