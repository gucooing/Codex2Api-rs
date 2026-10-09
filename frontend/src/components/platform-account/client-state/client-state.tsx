"use client";
import { BrowserClientState } from "@/components/platform-account/client-state/browser-client-state";
import { NamedClientState } from "@/components/platform-account/client-state/named-client-state";
import { ChatgptPluginRecords } from "@/components/providers/chatgpt/plugin-records";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldTitle } from "@/components/ui/field";
import { accountSections, clientRecordFields } from "@/lib/account-fields";
import { atPath } from "@/lib/domain";
import { date } from "@/lib/format";
import { useClientState } from "@/lib/platform-account/client-state";
import { scalar } from "@/lib/records";

export function ClientState({
  id,
  config,
}: {
  id: string;
  config: (typeof accountSections)[number];
}) {
  const { resource, rowsPath } = useClientState({ id, config });
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>{`${config.label}（只读）`}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {
          <>
            <CardDescription className="text-sm text-muted-foreground">
              来源：
              {(
                { client: "客户端", admin: "管理端服务策略", system: "系统" } as Record<
                  string,
                  string
                >
              )[resource.data?.write_origin ?? ""] ?? "未记录"}{" "}
              · 更新：{date(resource.data?.updated_at_ms)}
            </CardDescription>
            {(clientRecordFields[config.key] ?? []).length ? (
              <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
                {(clientRecordFields[config.key] ?? [])
                  .map((field) => ({
                    label: field.label,
                    value: scalar(atPath(resource.data?.value, field.path)),
                  }))
                  .map(({ label, value }) => (
                    <Field key={label}>
                      <FieldTitle>{label}</FieldTitle>
                      <FieldDescription>{value ?? "—"}</FieldDescription>
                    </Field>
                  ))}
              </FieldGroup>
            ) : config.key === "browser_settings" ? (
              <BrowserClientState value={resource.data?.value ?? null} path={rowsPath} />
            ) : config.key === "installed_plugins" ? (
              <ChatgptPluginRecords path={rowsPath} />
            ) : (
              <NamedClientState value={resource.data?.value ?? []} path={rowsPath} />
            )}
          </>
        }
      </CardContent>
    </Card>
  );
}
