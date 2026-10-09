"use client";
import { useFingerprintEditor } from "@/app/data/suppliers";
import { supplierChannel } from "@/components/providers";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldSet, FieldTitle } from "@/components/ui/field";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { type Supplier } from "@/lib/api";

export function FingerprintEditor({
  account,
  onSaved,
  disabled,
}: {
  account?: Supplier;
  onSaved: () => void;
  disabled: boolean;
}) {
  const { actions, proxies, setChanges, value, handleSubmit } = useFingerprintEditor({
    account,
    onSaved,
    disabled,
  });
  const ChannelFingerprintFields = supplierChannel(account?.provider_id).FingerprintFields;
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {"指纹与出站网络"}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
          {[
            { label: "Originator", value: account?.originator },
            { label: "安装标识", value: account?.installation_id },
            { label: "User-Agent", value: account?.user_agent },
          ].map(({ label, value }) => (
            <Field key={label}>
              <FieldTitle>{label}</FieldTitle>
              <FieldDescription>{value ?? "—"}</FieldDescription>
            </Field>
          ))}
        </FieldGroup>
        <Separator />
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("components\\suppliers.tsx:form:12")}
          onSubmit={(event) => handleSubmit(event)}
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={disabled || actions.isBusy("components\\suppliers.tsx:form:12")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      账户指纹
                    </CardTitle>
                  </div>
                  <ChannelFingerprintFields
                    value={value}
                    onChange={setChanges}
                    proxies={proxies.data?.items ?? []}
                  />
                </section>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            <Button
              type="submit"
              disabled={disabled || actions.isBusy("components\\suppliers.tsx:form:12")}
            >
              {actions.isBusy("components\\suppliers.tsx:form:12") && <Spinner />}
              {actions.isBusy("components\\suppliers.tsx:form:12") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  );
}
