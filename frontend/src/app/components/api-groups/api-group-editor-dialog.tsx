"use client";
import { useApiGroupEditor } from "@/app/data/api-groups";
import { ModelRulesTable } from "@/components/forms/model-rules-table";
import { ApiGroupSearchPrice } from "@/components/providers/chatgpt/billing";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import type { ApiGroup } from "@/lib/api-access";
import { providerLabel } from "@/lib/providers";

export function ApiGroupEditorDialog({
  group,
  ready,
  onClose,
  onSaved,
}: {
  group: ApiGroup;
  ready: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const {
    id,
    value,
    update,
    limits,
    setLimits,
    rules,
    setRule,
    modelSearch,
    setModelSearch,
    tags,
    models,
    toggleTag,
    toggleModel,
    selectVisible,
    busy,
    disabled,
    submit,
  } = useApiGroupEditor(group, ready, onSaved);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent
        className="flex max-h-[90dvh] flex-col sm:max-w-4xl"
        showCloseButton={!busy}
        onEscapeKeyDown={(event) => {
          if (busy) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (busy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{group.id ? "编辑 API 组" : "添加 API 组"}</DialogTitle>
          <DialogDescription>配置模型、计费倍率与每用户调用限制。</DialogDescription>
        </DialogHeader>
        <form noValidate className="flex min-h-0 flex-col gap-4" onSubmit={submit}>
          <Tabs defaultValue="group" className="min-h-0">
            <TabsList>
              <TabsTrigger value="group">组设置</TabsTrigger>
              <TabsTrigger value="models">模型与规则</TabsTrigger>
            </TabsList>
            <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-15rem)]">
              <TabsContent value="group" className="pr-3">
                <FieldSet disabled={disabled}>
                  <FieldGroup className="gap-4">
                    <Field>
                      <FieldLabel htmlFor={id + "-name"}>名称</FieldLabel>
                      <Input
                        id={id + "-name"}
                        required
                        maxLength={128}
                        value={value.name}
                        onChange={(event) => update("name", event.target.value)}
                      />
                    </Field>
                    <Field>
                      <FieldLabel htmlFor={id + "-description"}>介绍</FieldLabel>
                      <Textarea
                        id={id + "-description"}
                        value={value.description}
                        maxLength={2000}
                        rows={3}
                        onChange={(event) => update("description", event.target.value)}
                      />
                    </Field>
                    <Field orientation="horizontal">
                      <Switch
                        id={id + "-enabled"}
                        checked={value.enabled}
                        onCheckedChange={(enabled) => update("enabled", enabled)}
                      />
                      <FieldLabel htmlFor={id + "-enabled"}>启用 API 组</FieldLabel>
                    </Field>
                    <FieldSet>
                      <FieldLegend variant="label">关联标签</FieldLegend>
                      <div className="grid gap-2 sm:grid-cols-3">
                        {tags.data?.items.map((tag) => (
                          <Field key={tag.id} orientation="horizontal">
                            <Checkbox
                              id={id + "-tag-" + tag.id}
                              checked={value.tag_ids.includes(tag.id)}
                              onCheckedChange={(checked) => toggleTag(tag.id, checked === true)}
                            />
                            <FieldLabel htmlFor={id + "-tag-" + tag.id}>
                              {tag.name} · {providerLabel(tag.provider_id)}
                            </FieldLabel>
                          </Field>
                        ))}
                      </div>
                      {tags.ready && tags.data?.items.length === 0 && (
                        <FieldDescription>暂无标签。</FieldDescription>
                      )}
                    </FieldSet>
                    <ApiGroupSearchPrice
                      id={id + "-search-price"}
                      value={value.search_price_usd}
                      onChange={(price) => update("search_price_usd", price)}
                    />
                    <div className="grid gap-3 sm:grid-cols-3">
                      <Field>
                        <FieldLabel htmlFor={id + "-multiplier"}>组计费倍率</FieldLabel>
                        <Input
                          id={id + "-multiplier"}
                          inputMode="decimal"
                          required
                          value={value.multiplier}
                          onChange={(event) => update("multiplier", event.target.value)}
                        />
                      </Field>
                      <Field>
                        <FieldLabel htmlFor={id + "-concurrency"}>每用户组并发</FieldLabel>
                        <Input
                          id={id + "-concurrency"}
                          inputMode="numeric"
                          required
                          value={limits.concurrency_limit}
                          onChange={(event) =>
                            setLimits({ ...limits, concurrency_limit: event.target.value })
                          }
                        />
                      </Field>
                      <Field>
                        <FieldLabel htmlFor={id + "-rpm"}>每用户组 RPM</FieldLabel>
                        <Input
                          id={id + "-rpm"}
                          inputMode="numeric"
                          required
                          value={limits.rpm_limit}
                          onChange={(event) =>
                            setLimits({ ...limits, rpm_limit: event.target.value })
                          }
                        />
                      </Field>
                    </div>
                    <FieldDescription>
                      0 表示无限；同一用户在此组的所有密钥共同计数。
                    </FieldDescription>
                  </FieldGroup>
                </FieldSet>
              </TabsContent>
              <TabsContent value="models" className="pr-3">
                <ModelRulesTable
                  id={id}
                  options={models.data?.items}
                  ready={models.ready}
                  rules={rules}
                  baseMultiplier={value.multiplier}
                  search={modelSearch}
                  onSearch={setModelSearch}
                  onToggle={toggleModel}
                  onSelectVisible={selectVisible}
                  onRule={setRule}
                  disabled={busy}
                />
              </TabsContent>
            </ScrollArea>
          </Tabs>
          <DialogFooter>
            <Button type="button" variant="outline" disabled={busy} onClick={onClose}>
              取消
            </Button>
            <Button type="submit" disabled={disabled}>
              {busy && <Spinner />}保存
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
