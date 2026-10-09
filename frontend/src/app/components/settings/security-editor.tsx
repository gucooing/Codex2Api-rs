"use client";
import { useSecurityEditor } from "@/app/data/settings";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";

export function SecurityEditor({ username, disabled }: { username: string; disabled: boolean }) {
  const { fieldId, actions, form, update, handleSubmit } = useSecurityEditor({
    username,
    disabled,
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {"管理员凭据"}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <CardDescription className="text-sm text-muted-foreground">
          保存后当前管理员会话立即失效，需要重新登录。
        </CardDescription>
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("app\\settings\\page.tsx:form:4")}
          onSubmit={(event) => handleSubmit(event)}
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={disabled || actions.isBusy("app\\settings\\page.tsx:form:4")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      验证当前身份
                    </CardTitle>
                  </div>
                  <div className="grid sm:grid-cols-2 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-5" + "-" + encodeURIComponent(String("当前用户名"))
                        }
                      >
                        {"当前用户名"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-5" + "-" + encodeURIComponent(String("当前用户名"))}
                        aria-label={"当前用户名"}
                        required
                        value={form.old_username}
                        onChange={(e) => update("old_username", e.target.value)}
                      />
                    </Field>
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-6" + "-" + encodeURIComponent(String("当前密码"))
                        }
                      >
                        {"当前密码"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-6" + "-" + encodeURIComponent(String("当前密码"))}
                        aria-label={"当前密码"}
                        required
                        type="password"
                        autoComplete="current-password"
                        value={form.old_password}
                        onChange={(e) => update("old_password", e.target.value)}
                      />
                    </Field>
                  </div>
                </section>
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      更新登录凭据
                    </CardTitle>
                  </div>
                  <div className="grid sm:grid-cols-2 gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId + "-field-7" + "-" + encodeURIComponent(String("新用户名"))
                        }
                      >
                        {"新用户名"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-7" + "-" + encodeURIComponent(String("新用户名"))}
                        aria-label={"新用户名"}
                        required
                        value={form.new_username}
                        onChange={(e) => update("new_username", e.target.value)}
                      />
                    </Field>
                    <Field>
                      <FieldLabel
                        htmlFor={fieldId + "-field-8" + "-" + encodeURIComponent(String("新密码"))}
                      >
                        {"新密码"}
                      </FieldLabel>
                      <Input
                        id={fieldId + "-field-8" + "-" + encodeURIComponent(String("新密码"))}
                        aria-label={"新密码"}
                        aria-describedby={
                          fieldId +
                          "-field-8" +
                          "-" +
                          encodeURIComponent(String("新密码")) +
                          "-hint"
                        }
                        type="password"
                        autoComplete="new-password"
                        value={form.new_password}
                        onChange={(e) => update("new_password", e.target.value)}
                      />
                      {Boolean("留空保留原密码") && (
                        <FieldDescription
                          id={
                            fieldId +
                            "-field-8" +
                            "-" +
                            encodeURIComponent(String("新密码")) +
                            "-hint"
                          }
                        >
                          {"留空保留原密码"}
                        </FieldDescription>
                      )}
                    </Field>
                  </div>
                </section>
              </FieldGroup>
            </FieldSet>
          </ScrollArea>
          <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
            <Button
              type="submit"
              disabled={disabled || actions.isBusy("app\\settings\\page.tsx:form:4")}
            >
              {actions.isBusy("app\\settings\\page.tsx:form:4") && <Spinner />}
              {actions.isBusy("app\\settings\\page.tsx:form:4") ? "正在提交…" : "保存"}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  );
}
