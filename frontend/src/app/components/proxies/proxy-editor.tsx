"use client";
import { useProxyEditor } from "@/app/data/proxies";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { CardDescription, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { type Proxy } from "@/lib/api";
import { Info, X } from "lucide-react";

export function ProxyEditor({
  proxy,
  onClose,
  onSaved,
}: {
  proxy?: Proxy;
  onClose: () => void;
  onSaved: () => void;
}) {
  const {
    dialogFocus,
    fieldId,
    actions,
    value,
    passwordAction,
    setPasswordAction,
    update,
    handleSubmit,
  } = useProxyEditor({ proxy, onSaved });
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !actions.running.size) onClose();
      }}
    >
      <DialogContent
        {...dialogFocus}
        showCloseButton={false}
        className="flex max-h-[90dvh] min-h-0 flex-col sm:max-w-3xl"
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (actions.running.size) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (actions.running.size) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{proxy ? "编辑代理" : "添加代理"}</DialogTitle>
        </DialogHeader>
        <DialogClose asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="absolute right-4 top-4"
            aria-label="关闭"
            disabled={actions.running.size > 0}
          >
            <X />
          </Button>
        </DialogClose>
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("app\\proxies\\page.tsx:form:8")}
          onSubmit={(event) => handleSubmit(event)}
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={actions.isBusy("app\\proxies\\page.tsx:form:8")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      连接信息
                    </CardTitle>
                  </div>
                  <Field>
                    <FieldLabel
                      htmlFor={fieldId + "-field-9" + "-" + encodeURIComponent(String("名称"))}
                    >
                      {"名称"}
                    </FieldLabel>
                    <Input
                      id={fieldId + "-field-9" + "-" + encodeURIComponent(String("名称"))}
                      aria-label={"名称"}
                      value={value.name}
                      required
                      maxLength={128}
                      onChange={(event) => update("name", event.target.value)}
                    />
                  </Field>
                  <div className="grid sm:grid-cols-2 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={fieldId + "-field-10" + "-" + encodeURIComponent(String("协议"))}
                      >
                        {"协议"}
                      </FieldLabel>
                      <Select
                        value={value.protocol}
                        onValueChange={(next) =>
                          ((protocol) => update("protocol", protocol as Proxy["protocol"]))(
                            next ===
                              fieldId +
                                "-field-10" +
                                "-" +
                                encodeURIComponent(String("协议")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                      >
                        <SelectTrigger
                          id={fieldId + "-field-10" + "-" + encodeURIComponent(String("协议"))}
                          aria-label={"协议"}
                          data-empty={String(value.protocol) === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              ["http", "https", "socks5", "socks5h"]
                                .map((value) => ({
                                  value,
                                  label: value.toUpperCase(),
                                }))
                                .find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {["http", "https", "socks5", "socks5h"]
                            .map((value) => ({
                              value,
                              label: value.toUpperCase(),
                            }))
                            .map((option) => (
                              <SelectItem
                                key={option.value}
                                value={
                                  option.value ||
                                  fieldId +
                                    "-field-10" +
                                    "-" +
                                    encodeURIComponent(String("协议")) +
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
                        htmlFor={fieldId + "-field-11" + "-" + encodeURIComponent(String("端口"))}
                      >
                        {"端口"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-11" + "-" + encodeURIComponent(String("端口"))}
                        aria-label={"端口"}
                        value={value.port}
                        type="number"
                        min="1"
                        max="65535"
                        required
                        onChange={(event) => update("port", Number(event.target.value))}
                      />
                    </Field>
                  </div>
                  <Field>
                    <FieldLabel
                      htmlFor={fieldId + "-field-12" + "-" + encodeURIComponent(String("主机"))}
                    >
                      {"主机"}
                    </FieldLabel>
                    <Input
                      id={fieldId + "-field-12" + "-" + encodeURIComponent(String("主机"))}
                      aria-label={"主机"}
                      value={value.host}
                      required
                      onChange={(event) => update("host", event.target.value)}
                      placeholder="代理主机或 IP 地址"
                    />
                  </Field>
                </section>
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      身份验证
                    </CardTitle>
                    <CardDescription>
                      {proxy?.has_password
                        ? "已保存密码，出于安全原因不显示原值。"
                        : "此代理尚未配置密码。"}
                    </CardDescription>
                  </div>
                  <div className="grid sm:grid-cols-2 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={fieldId + "-field-13" + "-" + encodeURIComponent(String("用户名"))}
                      >
                        {"用户名"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-13" + "-" + encodeURIComponent(String("用户名"))}
                        aria-label={"用户名"}
                        value={value.username ?? ""}
                        autoComplete="off"
                        onChange={(event) => update("username", event.target.value)}
                      />
                    </Field>
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-14" + "-" + encodeURIComponent(String("密码操作"))
                        }
                      >
                        {"密码操作"}
                      </FieldLabel>
                      <Select
                        value={passwordAction}
                        onValueChange={(next) =>
                          setPasswordAction(
                            next ===
                              fieldId +
                                "-field-14" +
                                "-" +
                                encodeURIComponent(String("密码操作")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                      >
                        <SelectTrigger
                          id={fieldId + "-field-14" + "-" + encodeURIComponent(String("密码操作"))}
                          aria-label={"密码操作"}
                          data-empty={String(passwordAction) === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [
                                {
                                  value: "keep",
                                  label: proxy?.has_password ? "保留已保存密码" : "不设置密码",
                                },
                                {
                                  value: "replace",
                                  label: proxy?.has_password ? "更换密码" : "设置密码",
                                },
                                ...(proxy?.has_password
                                  ? [{ value: "remove", label: "清除已保存密码" }]
                                  : []),
                              ].find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[
                            {
                              value: "keep",
                              label: proxy?.has_password ? "保留已保存密码" : "不设置密码",
                            },
                            {
                              value: "replace",
                              label: proxy?.has_password ? "更换密码" : "设置密码",
                            },
                            ...(proxy?.has_password
                              ? [{ value: "remove", label: "清除已保存密码" }]
                              : []),
                          ].map((option) => (
                            <SelectItem
                              key={option.value}
                              value={
                                option.value ||
                                fieldId +
                                  "-field-14" +
                                  "-" +
                                  encodeURIComponent(String("密码操作")) +
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
                  </div>
                  {passwordAction === "replace" && (
                    <Field>
                      <FieldLabel
                        htmlFor={fieldId + "-field-15" + "-" + encodeURIComponent(String("新密码"))}
                      >
                        {"新密码"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-15" + "-" + encodeURIComponent(String("新密码"))}
                        aria-label={"新密码"}
                        value={value.password ?? ""}
                        required
                        type="password"
                        autoComplete="new-password"
                        onChange={(event) => update("password", event.target.value)}
                      />
                    </Field>
                  )}
                  {passwordAction === "remove" && (
                    <Alert>
                      <Info />
                      <AlertDescription>保存后将清除该代理的密码。</AlertDescription>
                    </Alert>
                  )}
                </section>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            {onClose && (
              <Button
                type="button"
                variant="outline"
                disabled={actions.isBusy("app\\proxies\\page.tsx:form:8")}
                onClick={onClose}
              >
                {"取消"}
              </Button>
            )}
            <Button type="submit" disabled={actions.isBusy("app\\proxies\\page.tsx:form:8")}>
              {actions.isBusy("app\\proxies\\page.tsx:form:8") && <Spinner />}
              {actions.isBusy("app\\proxies\\page.tsx:form:8") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </DialogContent>
    </Dialog>
  );
}
