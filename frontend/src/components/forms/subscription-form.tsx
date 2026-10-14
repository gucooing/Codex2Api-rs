"use client";
import { useSubscriptionForm } from "@/app/data/subscriptions";
import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { useActions } from "@/lib/actions";
import type { SubscriptionAccount } from "@/lib/api";
import { localDate } from "@/lib/domain";
import { userOptionLabel, type UserOption } from "@/lib/user-lookup";

export function SubscriptionForm({
  account,
  actions,
  onSaved,
  onCancel,
  editing = Boolean(account),
  disabled = false,
}: {
  account?: SubscriptionAccount;
  actions?: ReturnType<typeof useActions>;
  onSaved: () => void;
  onCancel?: () => void;
  editing?: boolean;
  disabled?: boolean;
}) {
  const {
    popupContainer,
    fieldId,
    value,
    update,
    selectProvider,
    selectPlan,
    busy,
    plans,
    planOptions,
    handleSubmit,
    users,
    chosenUser,
    setChosenUser,
  } = useSubscriptionForm({ account, actions, onSaved, editing, disabled });
  return (
    <form ref={popupContainer} noValidate onSubmit={handleSubmit}>
      <FieldSet disabled={disabled || busy || !plans.ready}>
        <FieldGroup className="grid gap-3 sm:grid-cols-2">
          <Field>
            <FieldLabel htmlFor={`${fieldId}-user`}>用户</FieldLabel>
            {editing ? (
              <Input id={`${fieldId}-user`} readOnly value={account?.username ?? ""} />
            ) : (
              <Combobox<UserOption>
                items={users.data?.items ?? []}
                value={chosenUser}
                open={users.open}
                onOpenChange={users.setOpen}
                onValueChange={(user) => {
                  setChosenUser(user);
                  update("user_id", user?.id ?? "");
                }}
                itemToStringLabel={userOptionLabel}
                filter={null}
              >
                <ComboboxInput
                  id={`${fieldId}-user`}
                  className="w-full"
                  placeholder="搜索用户"
                  onChange={(event) => users.setSearch(event.target.value)}
                  showClear
                />
                <ComboboxContent container={popupContainer}>
                  <ComboboxEmpty>没有匹配用户</ComboboxEmpty>
                  <ComboboxList>
                    {(user: UserOption) => (
                      <ComboboxItem key={user.id} value={user}>
                        {userOptionLabel(user)}
                      </ComboboxItem>
                    )}
                  </ComboboxList>
                </ComboboxContent>
              </Combobox>
            )}
          </Field>
          <Field>
            <FieldLabel htmlFor={`${fieldId}-provider`}>提供商</FieldLabel>
            <Select value={value.provider_id} disabled={editing} onValueChange={selectProvider}>
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
            <FieldLabel htmlFor={`${fieldId}-plan`}>套餐</FieldLabel>
            <Select value={value.plan_id} onValueChange={selectPlan}>
              <SelectTrigger id={`${fieldId}-plan`} className="w-full">
                <SelectValue placeholder="请选择套餐" />
              </SelectTrigger>
              <SelectContent position="popper">
                {planOptions.map((plan) => (
                  <SelectItem key={plan.id} value={plan.id}>
                    {plan.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor={`${fieldId}-expiry`}>到期时间（留空长期有效）</FieldLabel>
            <Input
              id={`${fieldId}-expiry`}
              type="datetime-local"
              value={value.subscription_expires_at ? localDate(value.subscription_expires_at) : ""}
              onChange={(event) => update("subscription_expires_at", event.target.value || null)}
            />
          </Field>
          <Field orientation="horizontal">
            <Switch
              id={`${fieldId}-enabled`}
              checked={value.enabled}
              onCheckedChange={(enabled) => update("enabled", enabled)}
            />
            <FieldLabel htmlFor={`${fieldId}-enabled`}>启用订阅</FieldLabel>
          </Field>
          {editing && (
            <Field orientation="horizontal">
              <Switch
                id={`${fieldId}-reissue`}
                checked={value.reissue}
                onCheckedChange={(reissue) => update("reissue", reissue)}
              />
              <FieldLabel htmlFor={`${fieldId}-reissue`}>重新发放，使用当前售价</FieldLabel>
            </Field>
          )}
        </FieldGroup>
        <div className="flex justify-end gap-2">
          {onCancel && (
            <Button type="button" variant="outline" onClick={onCancel}>
              取消
            </Button>
          )}
          <Button type="submit">{busy ? "保存中" : "保存"}</Button>
        </div>
      </FieldSet>
    </form>
  );
}
