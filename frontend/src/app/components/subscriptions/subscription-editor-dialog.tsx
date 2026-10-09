"use client";
import {
  SubscriptionEditorDialogProps,
  useSubscriptionEditorDialog,
} from "@/app/data/subscriptions";
import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { userOptionLabel, type UserOption } from "@/lib/user-lookup";

export function SubscriptionEditorDialog({
  editing,
  actions,
  setEditing,
  focus,
  editorUser,
  plans,
  resource,
  id,
  editorLookup,
  editorPlans,
}: SubscriptionEditorDialogProps) {
  const { handleSubmit } = useSubscriptionEditorDialog({
    editing,
    actions,
    setEditing,
    editorUser,
    plans,
    resource,
  });
  return (
    <Dialog
      open={!!editing}
      onOpenChange={(open) => {
        if (!open && !actions.isBusy("subscription")) setEditing(undefined);
      }}
    >
      <DialogContent
        {...focus}
        aria-describedby={undefined}
        className="max-h-[90dvh] overflow-y-auto"
      >
        <DialogHeader>
          <DialogTitle>{editing?.id ? "调整订阅" : "发放订阅"}</DialogTitle>
        </DialogHeader>
        <form noValidate onSubmit={(e) => handleSubmit(e)}>
          <FieldSet
            disabled={!editing || !plans.ready || !resource.ready || actions.isBusy("subscription")}
          >
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor={`${id}-user`}>用户</FieldLabel>
                <Combobox<UserOption>
                  items={editorLookup.data?.items ?? []}
                  value={editorUser.data?.user ?? null}
                  disabled={!!editing?.id}
                  filter={null}
                  open={editorLookup.open}
                  onOpenChange={editorLookup.setOpen}
                  itemToStringLabel={userOptionLabel}
                  itemToStringValue={(item) => item.id}
                  isItemEqualToValue={(item, value) => item.id === value.id}
                  onInputValueChange={(text, details) => {
                    if (details.reason === "input-change") editorLookup.setSearch(text);
                  }}
                  onValueChange={(item) =>
                    setEditing((v) => (v ? { ...v, user_id: item?.id ?? "" } : v))
                  }
                >
                  <ComboboxInput id={`${id}-user`} placeholder="搜索选择用户" />
                  <ComboboxContent>
                    <ComboboxEmpty>
                      {editorLookup.loading ? "加载中…" : "没有匹配用户"}
                    </ComboboxEmpty>
                    <ComboboxList>
                      {(item: UserOption) => (
                        <ComboboxItem key={item.id} value={item}>
                          {userOptionLabel(item)}
                        </ComboboxItem>
                      )}
                    </ComboboxList>
                  </ComboboxContent>
                </Combobox>
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-plan`}>套餐</FieldLabel>
                <Select
                  value={editing?.plan_id ?? ""}
                  onValueChange={(plan_id) => setEditing((v) => (v ? { ...v, plan_id } : v))}
                >
                  <SelectTrigger id={`${id}-plan`}>
                    <SelectValue placeholder="请选择套餐" />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    {editorPlans.data?.items.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name} · {p.provider_id}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-expiry`}>到期时间（留空长期有效）</FieldLabel>
                <Input
                  id={`${id}-expiry`}
                  type="datetime-local"
                  value={editing?.expires_at ?? ""}
                  onChange={(e) =>
                    setEditing((v) => (v ? { ...v, expires_at: e.target.value } : v))
                  }
                />
              </Field>
              <Field orientation="horizontal">
                <Switch
                  id={`${id}-reissue`}
                  checked={editing?.reissue ?? false}
                  onCheckedChange={(reissue) =>
                    setEditing((value) => (value ? { ...value, reissue } : value))
                  }
                />
                <FieldLabel htmlFor={`${id}-reissue`}>重新发放，重启周期并使用当前售价</FieldLabel>
              </Field>
              <Field orientation="horizontal">
                <Switch
                  id={`${id}-enabled`}
                  checked={editing?.enabled ?? false}
                  onCheckedChange={(enabled) => setEditing((v) => (v ? { ...v, enabled } : v))}
                />
                <FieldLabel htmlFor={`${id}-enabled`}>允许平台登录</FieldLabel>
              </Field>
              <DialogFooter>
                <Button type="submit" disabled={!editorUser.ready || !editorPlans.ready}>
                  保存
                </Button>
              </DialogFooter>
            </FieldGroup>
          </FieldSet>
        </form>
      </DialogContent>
    </Dialog>
  );
}
