"use client";
import { useGateway } from "@/app/data/settings";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { type GatewaySettings } from "@/lib/api";

export function Gateway() {
  const {
    fieldId,
    actions,
    resource,
    setValue,
    rulesText,
    setRulesText,
    rpmText,
    setRpmText,
    current,
    handleSubmit,
  } = useGateway();
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {"网关与限流规则"}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {!resource.ready && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={resource.reload}
            disabled={resource.refreshing}
          >
            {resource.refreshing ? <Spinner /> : null}重新加载
          </Button>
        )}
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("app\\settings\\page.tsx:form:1")}
          onSubmit={(event) => handleSubmit(event)}
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={!resource.ready || actions.isBusy("app\\settings\\page.tsx:form:1")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <Field>
                  <FieldLabel htmlFor={`${fieldId}-rpm`}>默认 RPM 限制</FieldLabel>
                  <Input
                    id={`${fieldId}-rpm`}
                    type="number"
                    min={0}
                    step={1}
                    value={resource.data ? (rpmText ?? current.default_rpm) : ""}
                    onChange={(event) => setRpmText(event.target.value)}
                  />
                  <FieldDescription>
                    每个虚拟账户每 60 秒最多允许的生成请求数，初始为 20，0
                    为无限。账户单独配置优先。
                  </FieldDescription>
                </Field>

                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      访问策略
                    </CardTitle>
                  </div>
                  <Field>
                    <FieldLabel
                      htmlFor={fieldId + "-field-2" + "-" + encodeURIComponent(String("名单模式"))}
                    >
                      {"名单模式"}
                    </FieldLabel>
                    <Select
                      value={resource.data ? current.ua_mode : ""}
                      onValueChange={(next) =>
                        ((ua_mode) =>
                          setValue({ ...current, ua_mode: ua_mode as GatewaySettings["ua_mode"] }))(
                          next ===
                            fieldId +
                              "-field-2" +
                              "-" +
                              encodeURIComponent(String("名单模式")) +
                              "-empty"
                            ? ""
                            : next,
                        )
                      }
                    >
                      <SelectTrigger
                        id={fieldId + "-field-2" + "-" + encodeURIComponent(String("名单模式"))}
                        aria-label={"名单模式"}
                        aria-describedby={
                          fieldId +
                          "-field-2" +
                          "-" +
                          encodeURIComponent(String("名单模式")) +
                          "-hint"
                        }
                        data-empty={String(current.ua_mode) === "" ? "true" : undefined}
                        className="w-full"
                      >
                        <SelectValue placeholder={resource.data ? undefined : "尚未加载"} />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        {[
                          { value: "blacklist", label: "黑名单 · 拒绝匹配的客户端" },
                          { value: "whitelist", label: "白名单 · 仅允许匹配的客户端" },
                        ].map((option) => (
                          <SelectItem
                            key={option.value}
                            value={
                              option.value ||
                              fieldId +
                                "-field-2" +
                                "-" +
                                encodeURIComponent(String("名单模式")) +
                                "-empty"
                            }
                            disabled={"disabled" in option && Boolean(option.disabled)}
                          >
                            {option.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    {Boolean(
                      current.ua_mode === "blacklist"
                        ? "匹配规则的请求会被拒绝，其余请求可继续访问。"
                        : "仅匹配规则的请求可继续访问，规则为空时全部拒绝。",
                    ) && (
                      <FieldDescription
                        id={
                          fieldId +
                          "-field-2" +
                          "-" +
                          encodeURIComponent(String("名单模式")) +
                          "-hint"
                        }
                      >
                        {current.ua_mode === "blacklist"
                          ? "匹配规则的请求会被拒绝，其余请求可继续访问。"
                          : "仅匹配规则的请求可继续访问，规则为空时全部拒绝。"}
                      </FieldDescription>
                    )}
                  </Field>
                  <Field>
                    <FieldLabel
                      htmlFor={fieldId + "-field-3" + "-" + encodeURIComponent(String("UA 规则"))}
                    >
                      {"UA 规则"}
                    </FieldLabel>
                    <Textarea
                      id={fieldId + "-field-3" + "-" + encodeURIComponent(String("UA 规则"))}
                      aria-label={"UA 规则"}
                      aria-describedby={
                        fieldId + "-field-3" + "-" + encodeURIComponent(String("UA 规则")) + "-hint"
                      }
                      rows={6}
                      value={rulesText ?? current.ua_rules.join("\n")}
                      onChange={(e) => setRulesText(e.target.value)}
                    />
                    {Boolean("每行一条，忽略大小写并按包含匹配；* 可匹配任意字符。") && (
                      <FieldDescription
                        id={
                          fieldId +
                          "-field-3" +
                          "-" +
                          encodeURIComponent(String("UA 规则")) +
                          "-hint"
                        }
                      >
                        {"每行一条，忽略大小写并按包含匹配；* 可匹配任意字符。"}
                      </FieldDescription>
                    )}
                  </Field>
                </section>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            <Button
              type="submit"
              disabled={!resource.ready || actions.isBusy("app\\settings\\page.tsx:form:1")}
            >
              {actions.isBusy("app\\settings\\page.tsx:form:1") && <Spinner />}
              {actions.isBusy("app\\settings\\page.tsx:form:1") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  );
}
