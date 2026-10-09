"use client";
import { useDesktop } from "@/app/data/settings";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
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

export function Desktop() {
  const { fieldId, actions, resource, proxies, setValue, current, handleSubmit } = useDesktop();
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {"Desktop 支持"}
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
          aria-busy={actions.isBusy("app\\settings\\page.tsx:form:9")}
          onSubmit={(event) => handleSubmit(event)}
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={!resource.ready || actions.isBusy("app\\settings\\page.tsx:form:9")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      公开资源
                    </CardTitle>
                  </div>
                  <div className="grid sm:grid-cols-2 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-10" +
                          "-" +
                          encodeURIComponent(String("公开资源出站代理"))
                        }
                      >
                        {"公开资源出站代理"}
                      </FieldLabel>
                      <Select
                        value={current.proxy_id ?? ""}
                        onValueChange={(next) =>
                          ((proxy_id) => setValue({ ...current, proxy_id: proxy_id || null }))(
                            next ===
                              fieldId +
                                "-field-10" +
                                "-" +
                                encodeURIComponent(String("公开资源出站代理")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                      >
                        <SelectTrigger
                          id={
                            fieldId +
                            "-field-10" +
                            "-" +
                            encodeURIComponent(String("公开资源出站代理"))
                          }
                          aria-label={"公开资源出站代理"}
                          data-empty={String(current.proxy_id ?? "") === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [
                                { value: "", label: "使用服务器默认网络" },
                                ...(proxies.data?.items.map((proxy) => ({
                                  value: proxy.id,
                                  label: `${proxy.name} · ${proxy.display_url}`,
                                })) ?? []),
                              ].find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[
                            { value: "", label: "使用服务器默认网络" },
                            ...(proxies.data?.items.map((proxy) => ({
                              value: proxy.id,
                              label: `${proxy.name} · ${proxy.display_url}`,
                            })) ?? []),
                          ].map((option) => (
                            <SelectItem
                              key={option.value}
                              value={
                                option.value ||
                                fieldId +
                                  "-field-10" +
                                  "-" +
                                  encodeURIComponent(String("公开资源出站代理")) +
                                  "-empty"
                              }
                              disabled={"disabled" in option && Boolean(option.disabled)}
                            >
                              {option.label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </Field>
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-11" +
                          "-" +
                          encodeURIComponent(String("公开资源缓存时间（分钟）"))
                        }
                      >
                        {"公开资源缓存时间（分钟）"}
                      </FieldLabel>
                      <Input
                        id={
                          fieldId +
                          "-field-11" +
                          "-" +
                          encodeURIComponent(String("公开资源缓存时间（分钟）"))
                        }
                        aria-label={"公开资源缓存时间（分钟）"}
                        required
                        type="number"
                        min="1"
                        max="1440"
                        value={resource.data ? current.resource_cache_minutes : ""}
                        onChange={(e) =>
                          setValue({ ...current, resource_cache_minutes: Number(e.target.value) })
                        }
                      />
                    </Field>
                  </div>
                </section>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            <Button
              type="submit"
              disabled={!resource.ready || actions.isBusy("app\\settings\\page.tsx:form:9")}
            >
              {actions.isBusy("app\\settings\\page.tsx:form:9") && <Spinner />}
              {actions.isBusy("app\\settings\\page.tsx:form:9") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  );
}
