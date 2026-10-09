"use client";
import { useConsumerForm } from "@/app/data/consumers";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
  FieldSet,
} from "@/components/ui/field";
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
import { Switch } from "@/components/ui/switch";
import { type Consumer } from "@/lib/api";
import { localDate } from "@/lib/domain";

export function ConsumerForm({
  account,
  onSaved,
  onCancel,
  editing = Boolean(account),
  disabled = false,
}: {
  account?: Consumer;
  editing?: boolean;
  disabled?: boolean;
  onSaved: () => void;
  onCancel?: () => void;
}) {
  const { fieldId, value, update, busy, plans, planOptions, handleSubmit } = useConsumerForm({
    account,
    onSaved,
    editing,
    disabled,
  });
  const fields = (
    <FieldSet disabled={disabled || busy} className="gap-3">
      <div
        className={
          onCancel ? "grid gap-3 sm:grid-cols-2" : "grid gap-3 sm:grid-cols-2 2xl:grid-cols-3"
        }
      >
        <Field>
          <FieldLabel htmlFor={`${fieldId}-name`}>账户名称</FieldLabel>
          <Input
            id={`${fieldId}-name`}
            required
            maxLength={128}
            value={value.name}
            onChange={(event) => update("name", event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-username`}>登录用户名</FieldLabel>
          <Input
            id={`${fieldId}-username`}
            required
            maxLength={128}
            autoComplete="off"
            value={value.username}
            onChange={(event) => update("username", event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-email`}>邮箱</FieldLabel>
          <Input
            id={`${fieldId}-email`}
            type="email"
            required
            maxLength={254}
            value={value.email}
            onChange={(event) => update("email", event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-provider`}>提供商</FieldLabel>
          <Select
            value={value.provider_id}
            onValueChange={(provider) => update("provider_id", provider)}
            disabled={editing || disabled || busy}
          >
            <SelectTrigger id={`${fieldId}-provider`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="chatgpt">ChatGPT</SelectItem>
              <SelectItem value="grok">Grok</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-password`}>{editing ? "新密码" : "登录密码"}</FieldLabel>
          <Input
            id={`${fieldId}-password`}
            type="password"
            autoComplete="new-password"
            required={!editing}
            placeholder={editing ? "留空保留现有密码" : undefined}
            value={value.password}
            onChange={(event) => update("password", event.target.value)}
          />
        </Field>
        <Field orientation="horizontal" className="self-end pb-1">
          <Switch
            id={`${fieldId}-enabled`}
            checked={value.enabled}
            onCheckedChange={(enabled) => update("enabled", enabled)}
          />
          <FieldContent>
            <FieldLabel htmlFor={`${fieldId}-enabled`}>允许账户登录</FieldLabel>
          </FieldContent>
        </Field>
      </div>
      <FieldSeparator />
      <div className="grid gap-3 sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${fieldId}-plan`}>订阅套餐</FieldLabel>
          <Select
            value={value.plan_id}
            onValueChange={(plan) => {
              if (!plan || disabled || busy || !plans.ready) return;
              update("plan_id", plan === `${fieldId}-empty-plan` ? "" : plan);
            }}
            disabled={disabled || busy || !plans.ready}
          >
            <SelectTrigger
              id={`${fieldId}-plan`}
              className="w-full"
              data-required="true"
              data-empty={!value.plan_id ? "true" : undefined}
            >
              <SelectValue placeholder={account?.plan_name || "请选择套餐"} />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value={`${fieldId}-empty-plan`}>
                {plans.loading ? "正在加载套餐…" : "请选择套餐"}
              </SelectItem>
              {planOptions.map((plan) => (
                <SelectItem key={plan.id} value={plan.id}>
                  {plan.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${fieldId}-expires`}>订阅到期时间</FieldLabel>
          <Input
            id={`${fieldId}-expires`}
            type="datetime-local"
            value={localDate(value.subscription_expires_at)}
            onChange={(event) =>
              update(
                "subscription_expires_at",
                event.target.value ? new Date(event.target.value).toISOString() : null,
              )
            }
          />
        </Field>
      </div>
    </FieldSet>
  );
  return (
    <form
      noValidate
      className="flex min-h-0 flex-col gap-3"
      aria-busy={busy}
      onSubmit={(event) => handleSubmit(event)}
    >
      {onCancel ? (
        <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
          {fields}
        </ScrollArea>
      ) : (
        fields
      )}
      <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
        {onCancel && (
          <Button type="button" variant="outline" disabled={disabled || busy} onClick={onCancel}>
            取消
          </Button>
        )}
        <Button type="submit" disabled={disabled || busy || !plans.ready}>
          {busy && <Spinner />}
          {busy ? "正在提交…" : editing ? "保存账户与订阅" : "创建账户"}
        </Button>
      </FieldGroup>
    </form>
  );
}
