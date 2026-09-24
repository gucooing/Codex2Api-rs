"use client";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useTablePagination } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from "lucide-react";

import { CardDescription } from "@/components/ui/card";

import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardAction } from "@/components/ui/card";
import { Empty, EmptyHeader, EmptyDescription, EmptyMedia } from "@/components/ui/empty";
import { Inbox, Info } from "lucide-react";
import { useErrorToast, useActions } from "@/lib/actions";
import {
  FieldSet,
  FieldGroup,
  FieldLegend,
  Field,
  FieldLabel,
  FieldDescription,
  FieldTitle,
  FieldContent,
} from "@/components/ui/field";
import { useId } from "react";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
} from "@/components/ui/table";
import { date } from "@/lib/format";
import { useState } from "react";
import { ChevronDown, RotateCcw } from "lucide-react";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  request,
  type BusinessField,
  type Config,
  type Json,
  type List,
  type Model,
} from "@/lib/api";
import { allowedFields, atPath, canEditConfig, withPath } from "@/lib/domain";
import { useResource } from "@/lib/hooks";

import { scalar } from "@/lib/records";
import { accountConfigGroups, accountSections, clientRecordFields } from "@/lib/account-fields";

const spec = (
  path: string,
  label: string,
  kind: BusinessField["kind"] = "string",
  optional = true,
): BusinessField => ({
  path: path.split("."),
  label,
  kind,
  choices: [],
  group: "服务设置",
  description: "",
  optional,
});
const serviceFields: Record<string, BusinessField[]> = {
  profile: [spec("picture", "头像地址", "url"), spec("bio", "个人简介", "text")],
  age: [
    spec("is_adult", "是否成年", "boolean"),
    spec("has_verified_age_or_dob", "已记录年龄资料", "boolean", false),
  ],
  trusted_contact: [spec("enabled", "开放信任联系人功能", "boolean", false)],
  referrals: [spec("should_show", "显示邀请入口", "boolean", false)],
  gift_credits: [spec("eligible", "显示赠送入口", "boolean", false)],
  first_party: [
    spec("finances", "显示财务入口", "boolean", false),
    spec("health_eligibility.sidebar_visible", "显示健康侧栏入口", "boolean", false),
  ],
  sites: [spec("enabled", "开放站点功能", "boolean", false)],
  computer_use_policy: [
    spec("browser_enabled", "允许浏览器功能", "boolean", false),
    spec("computer_enabled", "允许电脑操控功能", "boolean", false),
  ],
  desktop_model_policy: [
    spec("reasoning_settings_enabled", "显示推理强度设置", "boolean", false),
    spec("ultra_effort_available", "提供 Ultra 推理强度", "boolean", false),
  ],
};
export function ConfigPanel({ id, group }: { id: string; group: string }) {
  const resource = useResource<List<Config>>(`/consumers/${id}/configs`);
  const category = accountConfigGroups.find((item) => item.key === group);
  const items = (category?.sections ?? []).flatMap((key) => {
    const section = accountSections.find((item) => item.key === key);
    if (!section || !canEditConfig(section.key, section.readonly)) return [];
    const stored = resource.data?.items.find((item) => item.key === key);
    return [
      {
        ...section,
        fields: allowedFields(
          key,
          section.fields.length ? section.fields : (serviceFields[key] ?? []),
        ),
        value: stored?.value ?? null,
        revision: stored?.revision ?? 0,
        updated_at_ms: stored?.updated_at_ms ?? 0,
        readonly: section.readonly || Boolean(stored?.readonly),
      },
    ];
  });
  useErrorToast(resource.error);
  return (
    <ConfigForm
      key={`${id}:${group}`}
      id={id}
      group={group}
      title={category?.label ?? "账户设置"}
      items={items}
      disabled={
        !resource.ready ||
        items.some(
          (item) =>
            item.readonly || !resource.data?.items.some((stored) => stored.key === item.key),
        )
      }
      onSaved={resource.reload}
    />
  );
}
function ConfigForm({
  id,
  group,
  title,
  items,
  disabled,
  onSaved,
}: {
  id: string;
  group: string;
  title: string;
  items: Config[];
  disabled: boolean;
  onSaved: () => void;
}) {
  const fieldId = useId();
  const actions = useActions();
  const actionKey = `consumer-settings:${id}:${group}`;
  const busy = actions.isBusy(actionKey);
  const [drafts, setDrafts] = useState<Record<string, { value: Json; revision: number }>>({});
  const [committed, setCommitted] = useState<Record<string, { value: Json; revision: number }>>({});
  const configs = items.map((item) =>
    committed[item.key]?.revision > item.revision ? { ...item, ...committed[item.key] } : item,
  );
  const models = useResource<List<Model>>(
    items.some((item) => item.key === "conversation_metadata") ? "/models" : null,
  );
  useErrorToast(models.error);
  const valueOf = (config: Config) =>
    drafts[config.key] ? drafts[config.key].value : config.value;
  const change = (config: Config, value: Json) =>
    setDrafts((current) => ({
      ...current,
      [config.key]: { value, revision: current[config.key]?.revision ?? config.revision },
    }));
  const pending = configs.filter(
    (item) =>
      drafts[item.key] && JSON.stringify(drafts[item.key].value) !== JSON.stringify(item.value),
  );
  const switches = configs.flatMap((config) =>
    config.fields.filter((field) => field.kind === "boolean").map((field) => ({ config, field })),
  );
  const fields = configs.filter((item) => item.fields.some((field) => field.kind !== "boolean"));
  const additionalFields = fields.map((config) => (
    <BusinessFields
      key={config.key}
      config={config}
      fields={config.fields.filter((field) => field.kind !== "boolean")}
      value={valueOf(config)}
      onChange={(value) => change(config, value)}
      models={models.data?.items.filter((model) => model.enabled) ?? []}
    />
  ));
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {title}
        </CardTitle>
        <CardAction className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={busy || !Object.keys(drafts).length}
            onClick={() => setDrafts({})}
          >
            重置修改
          </Button>
          <Button
            type="submit"
            form={`${fieldId}-form`}
            size="sm"
            disabled={disabled || busy || !pending.length}
          >
            {busy && <Spinner />}
            {busy ? "正在保存…" : "保存设置"}
          </Button>
        </CardAction>
        {switches.length > 0 && (
          <CardDescription>统一保存修改；标记为“默认”的选项尚未指定开启或关闭。</CardDescription>
        )}
      </CardHeader>
      <CardContent>
        <form
          id={`${fieldId}-form`}
          noValidate
          aria-busy={busy}
          onSubmit={(event) =>
            actions.submit(
              event,
              actionKey,
              async () => {
                if (disabled) throw new Error("请先加载账户设置");
                let savedCount = 0;
                const failures: string[] = [];
                for (const config of pending) {
                  try {
                    const saved = await request<{ value: Json; revision: number }>(
                      `/consumers/${id}/config/${config.key}`,
                      {
                        method: "PUT",
                        body: drafts[config.key],
                      },
                    );
                    setCommitted((current) => ({ ...current, [config.key]: saved }));
                    savedCount++;
                    setDrafts((current) => {
                      const next = { ...current };
                      delete next[config.key];
                      return next;
                    });
                  } catch (error) {
                    failures.push(
                      `${config.label}：${error instanceof Error ? error.message : "保存失败"}`,
                    );
                  }
                }
                onSaved();
                if (failures.length)
                  throw new Error(
                    `${savedCount ? "部分修改已保存。" : ""}未保存的修改已保留。${failures.join("；")}`,
                  );
              },
              "设置已保存",
            )
          }
        >
          <FieldSet disabled={disabled || busy} className="min-w-0 gap-4">
            {switches.length > 0 && (
              <FieldGroup className="grid gap-x-6 gap-y-4 md:grid-cols-2 xl:grid-cols-3">
                {switches.map(({ config, field }) => {
                  const value = valueOf(config);
                  const current = atPath(value, field.path);
                  const controlId = `${fieldId}-${config.key}-${field.path.join("-")}`;
                  return (
                    <Field key={controlId} orientation="horizontal">
                      <FieldContent>
                        <FieldLabel htmlFor={controlId}>{field.label}</FieldLabel>
                        {field.description && (
                          <FieldDescription id={`${controlId}-hint`} className="text-xs">
                            {field.description}
                          </FieldDescription>
                        )}
                      </FieldContent>
                      <div className="flex shrink-0 items-center gap-2">
                        {current == null && <Badge variant="outline">默认</Badge>}
                        <Switch
                          id={controlId}
                          checked={current === true}
                          aria-describedby={field.description ? `${controlId}-hint` : undefined}
                          onCheckedChange={(checked) =>
                            change(config, withPath(value, field.path, checked))
                          }
                        />
                        {field.optional && current != null && (
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            aria-label={`${field.label}：恢复默认`}
                            title="恢复默认"
                            onClick={() =>
                              change(
                                config,
                                withPath(
                                  value,
                                  field.path,
                                  config.key === "age" ? null : undefined,
                                ),
                              )
                            }
                          >
                            <RotateCcw />
                          </Button>
                        )}
                      </div>
                    </Field>
                  );
                })}
              </FieldGroup>
            )}
            {additionalFields.length > 0 &&
              (group === "features" ? (
                <Collapsible>
                  <CollapsibleTrigger asChild>
                    <Button type="button" variant="outline" size="sm">
                      更多功能选项
                      <ChevronDown />
                    </Button>
                  </CollapsibleTrigger>
                  <CollapsibleContent className="pt-3">{additionalFields}</CollapsibleContent>
                </Collapsible>
              ) : (
                additionalFields
              ))}
            {configs
              .filter((config) => !config.fields.length)
              .map((config) => (
                <FieldSet key={config.key}>
                  {items.length > 1 && <FieldLegend>{config.label}</FieldLegend>}
                  <ComplexService
                    id={id}
                    config={config}
                    value={valueOf(config)}
                    onChange={(value) => change(config, value)}
                  />
                </FieldSet>
              ))}
          </FieldSet>
        </form>
      </CardContent>
    </Card>
  );
}
function BusinessFields({
  fields,
  value,
  onChange,
  config,
  models,
}: {
  fields: BusinessField[];
  value: Json;
  onChange: (next: Json) => void;
  config: Config;
  models: Model[];
}) {
  const fieldId = useId();
  const visibleFields = fields.filter(
    (field) =>
      config.key !== "account_settings" ||
      field.path[0] !== "usage_limit_increase_request" ||
      field.path[1] === "kind" ||
      atPath(value, ["usage_limit_increase_request", "kind"]) === "custom",
  );
  const groups = [...new Set(visibleFields.map((field) => field.group))];
  return (
    <FieldGroup className="gap-4">
      {groups.map((group) => (
        <FieldSet key={group} className="gap-3">
          {groups.length > 1 && <FieldLegend>{group}</FieldLegend>}
          <div className="grid items-start gap-3 sm:grid-cols-2 xl:grid-cols-3">
            {visibleFields
              .filter((field) => field.group === group)
              .map((field) => {
                const current = atPath(value, field.path);
                const controlId = `${fieldId}-${field.path.join("-")}`;
                const update = (next: Json | undefined) =>
                  onChange(withPath(value, field.path, next));
                const modelChoices =
                  field.path.join(".") === "default_model_slug"
                    ? models.map((model) => [model.model, model.model] as [string, string])
                    : undefined;
                return (
                  <Field
                    key={controlId}
                    className={field.kind === "text" ? "sm:col-span-2" : undefined}
                  >
                    <FieldLabel htmlFor={controlId}>{field.label}</FieldLabel>
                    {field.kind === "select" || modelChoices ? (
                      <Select
                        value={current == null ? "" : String(current)}
                        onValueChange={(next) => {
                          if (!next) return;
                          update(
                            next === `${controlId}-default`
                              ? modelChoices
                                ? null
                                : undefined
                              : next,
                          );
                        }}
                      >
                        <SelectTrigger
                          id={controlId}
                          aria-describedby={field.description ? `${controlId}-hint` : undefined}
                          className="w-full"
                        >
                          <SelectValue placeholder={field.optional ? "使用默认设置" : "请选择"} />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {field.optional && (
                            <SelectItem value={`${controlId}-default`}>使用默认设置</SelectItem>
                          )}
                          {(modelChoices ?? field.choices).map(([key, label]) => (
                            <SelectItem key={key} value={key}>
                              {label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    ) : field.kind === "text" ? (
                      <Textarea
                        id={controlId}
                        aria-describedby={field.description ? `${controlId}-hint` : undefined}
                        rows={2}
                        maxLength={2000}
                        value={typeof current === "string" ? current : ""}
                        onChange={(event) => update(event.target.value)}
                      />
                    ) : (
                      <Input
                        id={controlId}
                        aria-describedby={field.description ? `${controlId}-hint` : undefined}
                        type={
                          field.kind === "integer"
                            ? "number"
                            : field.kind === "date"
                              ? "date"
                              : field.kind === "url"
                                ? "url"
                                : "text"
                        }
                        min={field.kind === "integer" ? 1 : undefined}
                        max={field.kind === "integer" ? 1000000 : undefined}
                        step={field.kind === "integer" ? 1 : undefined}
                        maxLength={2000}
                        value={
                          typeof current === "string" || typeof current === "number" ? current : ""
                        }
                        onChange={(event) =>
                          update(
                            event.target.value === ""
                              ? config.key === "profile" && field.path[0] === "picture"
                                ? null
                                : field.optional
                                  ? undefined
                                  : ""
                              : field.kind === "integer"
                                ? Number(event.target.value)
                                : event.target.value,
                          )
                        }
                      />
                    )}
                    {field.description && (
                      <FieldDescription id={`${controlId}-hint`} className="text-xs">
                        {field.description}
                      </FieldDescription>
                    )}
                  </Field>
                );
              })}
          </div>
        </FieldSet>
      ))}
    </FieldGroup>
  );
}
const obj = (value: Json | undefined): { [key: string]: Json } =>
  value && typeof value === "object" && !Array.isArray(value) ? value : {};
function ComplexService({
  config,
  id,
  value,
  onChange,
}: {
  config: Config;
  id: string;
  value: Json;
  onChange: (next: Json) => void;
}) {
  const fieldId = useId();
  const plugins = useResource<List<{ id: string; name: string }>>(
    config.key === "system_hints" ? `/consumers/${id}/plugins` : null,
  );
  const connectors = useResource<List<{ id: string; name: string }>>(
    config.key === "system_hints" ? `/consumers/${id}/connectors` : null,
  );
  useErrorToast(plugins.error);
  useErrorToast(connectors.error);
  const root = obj(value);
  if (config.key === "system_hints" || config.key === "workspace_messages") {
    const hints = config.key === "system_hints";
    const key = hints ? "system_hints" : "messages";
    const rows = Array.isArray(root[key]) ? (root[key] as Json[]) : [];
    const change = (index: number, patch: Record<string, Json>) =>
      onChange({
        ...root,
        [key]: rows.map((row, i) => (i === index ? { ...obj(row), ...patch } : row)),
      });
    return (
      <div className="space-y-3">
        {rows.map((raw, index) => {
          const row = obj(raw);
          const kind = String(row.hint_kind ?? "basic");
          const choices = kind === "plugin" ? plugins.data?.items : connectors.data?.items;
          return (
            <section className="space-y-3" key={index}>
              <div className="flex flex-wrap items-center gap-2">
                <strong>
                  {hints ? "系统提示" : "工作区消息"} {index + 1}
                </strong>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => onChange({ ...root, [key]: rows.filter((_, i) => i !== index) })}
                >
                  移除
                </Button>
              </div>
              {hints ? (
                <>
                  <div className="grid gap-3">
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-8" +
                          "-" +
                          String(index) +
                          "-" +
                          encodeURIComponent(String("标题"))
                        }
                      >
                        {"标题"}
                      </FieldLabel>
                      <Input
                        id={
                          fieldId +
                          "-field-8" +
                          "-" +
                          String(index) +
                          "-" +
                          encodeURIComponent(String("标题"))
                        }
                        aria-label={"标题"}
                        required
                        value={String(row.title ?? "")}
                        onChange={(e) => change(index, { title: e.target.value })}
                      />
                    </Field>
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-9" +
                          "-" +
                          String(index) +
                          "-" +
                          encodeURIComponent(String("提示类型"))
                        }
                      >
                        {"提示类型"}
                      </FieldLabel>
                      <Select
                        value={kind}
                        onValueChange={(next) =>
                          ((hint_kind) => change(index, { hint_kind, resource_id: "" }))(
                            next ===
                              fieldId +
                                "-field-9" +
                                "-" +
                                String(index) +
                                "-" +
                                encodeURIComponent(String("提示类型")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                      >
                        <SelectTrigger
                          id={
                            fieldId +
                            "-field-9" +
                            "-" +
                            String(index) +
                            "-" +
                            encodeURIComponent(String("提示类型"))
                          }
                          aria-label={"提示类型"}
                          data-required={false ? "true" : undefined}
                          data-empty={String(kind) === "" ? "true" : undefined}
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [
                                { value: "basic", label: "普通提示" },
                                { value: "plugin", label: "插件" },
                                { value: "connector", label: "连接器" },
                              ].find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[
                            { value: "basic", label: "普通提示" },
                            { value: "plugin", label: "插件" },
                            { value: "connector", label: "连接器" },
                          ].map((option) => (
                            <SelectItem
                              key={option.value}
                              value={
                                option.value ||
                                fieldId +
                                  "-field-9" +
                                  "-" +
                                  String(index) +
                                  "-" +
                                  encodeURIComponent(String("提示类型")) +
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
                  <Field>
                    <FieldLabel
                      htmlFor={
                        fieldId +
                        "-field-10" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("说明"))
                      }
                    >
                      {"说明"}
                    </FieldLabel>
                    <Textarea
                      id={
                        fieldId +
                        "-field-10" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("说明"))
                      }
                      aria-label={"说明"}
                      value={String(row.description ?? "")}
                      onChange={(e) => change(index, { description: e.target.value })}
                    />
                  </Field>
                  {kind === "basic" ? (
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-11" +
                          "-" +
                          String(index) +
                          "-" +
                          encodeURIComponent(String("提示动作"))
                        }
                      >
                        {"提示动作"}
                      </FieldLabel>
                      <Select
                        value={String(row.basic_action ?? "search")}
                        onValueChange={(next) =>
                          ((basic_action) => change(index, { basic_action }))(
                            next ===
                              fieldId +
                                "-field-11" +
                                "-" +
                                String(index) +
                                "-" +
                                encodeURIComponent(String("提示动作")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                      >
                        <SelectTrigger
                          id={
                            fieldId +
                            "-field-11" +
                            "-" +
                            String(index) +
                            "-" +
                            encodeURIComponent(String("提示动作"))
                          }
                          aria-label={"提示动作"}
                          data-required={false ? "true" : undefined}
                          data-empty={
                            String(String(row.basic_action ?? "search")) === "" ? "true" : undefined
                          }
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [
                                { value: "search", label: "搜索网页" },
                                { value: "picture_v2", label: "生成图片" },
                                { value: "tatertot", label: "学习辅导" },
                              ].find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[
                            { value: "search", label: "搜索网页" },
                            { value: "picture_v2", label: "生成图片" },
                            { value: "tatertot", label: "学习辅导" },
                          ].map((option) => (
                            <SelectItem
                              key={option.value}
                              value={
                                option.value ||
                                fieldId +
                                  "-field-11" +
                                  "-" +
                                  String(index) +
                                  "-" +
                                  encodeURIComponent(String("提示动作")) +
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
                  ) : (
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-12" +
                          "-" +
                          String(index) +
                          "-" +
                          encodeURIComponent(String("关联资源"))
                        }
                      >
                        {"关联资源"}
                      </FieldLabel>
                      <Select
                        value={String(row.resource_id ?? "")}
                        onValueChange={(next) =>
                          ((resource_id) => change(index, { resource_id }))(
                            next ===
                              fieldId +
                                "-field-12" +
                                "-" +
                                String(index) +
                                "-" +
                                encodeURIComponent(String("关联资源")) +
                                "-empty"
                              ? ""
                              : next,
                          )
                        }
                        required={true}
                      >
                        <SelectTrigger
                          id={
                            fieldId +
                            "-field-12" +
                            "-" +
                            String(index) +
                            "-" +
                            encodeURIComponent(String("关联资源"))
                          }
                          aria-label={"关联资源"}
                          data-required={true ? "true" : undefined}
                          data-empty={
                            String(String(row.resource_id ?? "")) === "" ? "true" : undefined
                          }
                          className="w-full"
                        >
                          <SelectValue
                            placeholder={
                              [
                                { value: "", label: "请选择已有资源" },
                                ...(choices?.map((item) => ({
                                  value: item.id,
                                  label: item.name || item.id,
                                })) ?? []),
                              ].find((option) => option.value === "")?.label ?? "请选择"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent position="popper">
                          {[
                            { value: "", label: "请选择已有资源" },
                            ...(choices?.map((item) => ({
                              value: item.id,
                              label: item.name || item.id,
                            })) ?? []),
                          ].map((option) => (
                            <SelectItem
                              key={option.value}
                              value={
                                option.value ||
                                fieldId +
                                  "-field-12" +
                                  "-" +
                                  String(index) +
                                  "-" +
                                  encodeURIComponent(String("关联资源")) +
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
                  )}
                </>
              ) : (
                <>
                  <Field>
                    <FieldLabel
                      htmlFor={
                        fieldId +
                        "-field-13" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("消息类型"))
                      }
                    >
                      {"消息类型"}
                    </FieldLabel>
                    <Input
                      id={
                        fieldId +
                        "-field-13" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("消息类型"))
                      }
                      aria-label={"消息类型"}
                      required
                      value={String(row.message_type ?? "")}
                      onChange={(e) => change(index, { message_type: e.target.value })}
                    />
                  </Field>
                  <Field>
                    <FieldLabel
                      htmlFor={
                        fieldId +
                        "-field-14" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("消息内容"))
                      }
                    >
                      {"消息内容"}
                    </FieldLabel>
                    <Textarea
                      id={
                        fieldId +
                        "-field-14" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("消息内容"))
                      }
                      aria-label={"消息内容"}
                      required
                      value={String(row.message_body ?? "")}
                      onChange={(e) => change(index, { message_body: e.target.value })}
                    />
                  </Field>
                </>
              )}
            </section>
          );
        })}
        <Button
          type="button"
          variant="outline"
          onClick={() =>
            onChange({
              ...root,
              [key]: [
                ...rows,
                hints
                  ? {
                      title: "",
                      description: "",
                      hint_kind: "basic",
                      basic_action: "search",
                      resource_id: "",
                    }
                  : { message_type: "", message_body: "" },
              ],
            })
          }
        >
          添加{hints ? "提示" : "消息"}
        </Button>
      </div>
    );
  }
  if (config.key === "beacons" || config.key === "discount_offer") {
    const beacon = config.key === "beacons";
    const key = beacon ? "beacon_ui_response" : "offer";
    const entry = root[key];
    const row = obj(entry);
    const patch = (next: Record<string, Json>) => onChange({ ...root, [key]: { ...row, ...next } });
    if (!entry)
      return (
        <Button
          type="button"
          variant="outline"
          onClick={() =>
            onChange({
              ...root,
              [key]: beacon
                ? { title: "", description: "", presentation: "banner", buttons: [] }
                : { title: "", description: "" },
            })
          }
        >
          添加{beacon ? "公告" : "优惠展示"}
        </Button>
      );
    const buttons = Array.isArray(row.buttons) ? row.buttons : [];
    return (
      <div className="space-y-4">
        <Field>
          <FieldLabel htmlFor={fieldId + "-field-15" + "-" + encodeURIComponent(String("标题"))}>
            {"标题"}
          </FieldLabel>
          <Input
            id={fieldId + "-field-15" + "-" + encodeURIComponent(String("标题"))}
            aria-label={"标题"}
            required
            value={String(row.title ?? "")}
            onChange={(e) => patch({ title: e.target.value })}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={fieldId + "-field-16" + "-" + encodeURIComponent(String("正文"))}>
            {"正文"}
          </FieldLabel>
          <Textarea
            id={fieldId + "-field-16" + "-" + encodeURIComponent(String("正文"))}
            aria-label={"正文"}
            value={String(row.description ?? "")}
            onChange={(e) => patch({ description: e.target.value })}
          />
        </Field>
        {beacon && (
          <>
            <Field>
              <FieldLabel
                htmlFor={fieldId + "-field-17" + "-" + encodeURIComponent(String("展示形式"))}
              >
                {"展示形式"}
              </FieldLabel>
              <Select
                value={String(row.presentation ?? "banner")}
                onValueChange={(next) =>
                  ((presentation) => patch({ presentation }))(
                    next ===
                      fieldId +
                        "-field-17" +
                        "-" +
                        encodeURIComponent(String("展示形式")) +
                        "-empty"
                      ? ""
                      : next,
                  )
                }
              >
                <SelectTrigger
                  id={fieldId + "-field-17" + "-" + encodeURIComponent(String("展示形式"))}
                  aria-label={"展示形式"}
                  data-required={false ? "true" : undefined}
                  data-empty={
                    String(String(row.presentation ?? "banner")) === "" ? "true" : undefined
                  }
                  className="w-full"
                >
                  <SelectValue
                    placeholder={
                      [
                        { value: "banner", label: "横幅" },
                        { value: "modal", label: "弹窗" },
                      ].find((option) => option.value === "")?.label ?? "请选择"
                    }
                  />
                </SelectTrigger>
                <SelectContent position="popper">
                  {[
                    { value: "banner", label: "横幅" },
                    { value: "modal", label: "弹窗" },
                  ].map((option) => (
                    <SelectItem
                      key={option.value}
                      value={
                        option.value ||
                        fieldId +
                          "-field-17" +
                          "-" +
                          encodeURIComponent(String("展示形式")) +
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
            {buttons.map((raw, index) => {
              const button = obj(raw);
              const change = (next: Record<string, Json>) =>
                patch({
                  buttons: buttons.map((item, i) => (i === index ? { ...button, ...next } : item)),
                });
              return (
                <div className="space-y-3 space-y-4" key={index}>
                  <Field>
                    <FieldLabel
                      htmlFor={
                        fieldId +
                        "-field-18" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("按钮文字"))
                      }
                    >
                      {"按钮文字"}
                    </FieldLabel>
                    <Input
                      id={
                        fieldId +
                        "-field-18" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("按钮文字"))
                      }
                      aria-label={"按钮文字"}
                      required
                      value={String(button.text ?? "")}
                      onChange={(e) => change({ text: e.target.value })}
                    />
                  </Field>
                  <Field>
                    <FieldLabel
                      htmlFor={
                        fieldId +
                        "-field-19" +
                        "-" +
                        String(index) +
                        "-" +
                        encodeURIComponent(String("按钮动作"))
                      }
                    >
                      {"按钮动作"}
                    </FieldLabel>
                    <Select
                      value={String(button.action ?? "dismiss")}
                      onValueChange={(next) =>
                        ((action) => change({ action }))(
                          next ===
                            fieldId +
                              "-field-19" +
                              "-" +
                              String(index) +
                              "-" +
                              encodeURIComponent(String("按钮动作")) +
                              "-empty"
                            ? ""
                            : next,
                        )
                      }
                    >
                      <SelectTrigger
                        id={
                          fieldId +
                          "-field-19" +
                          "-" +
                          String(index) +
                          "-" +
                          encodeURIComponent(String("按钮动作"))
                        }
                        aria-label={"按钮动作"}
                        data-required={false ? "true" : undefined}
                        data-empty={
                          String(String(button.action ?? "dismiss")) === "" ? "true" : undefined
                        }
                        className="w-full"
                      >
                        <SelectValue
                          placeholder={
                            [
                              { value: "dismiss", label: "关闭公告" },
                              { value: "open_url", label: "打开链接" },
                            ].find((option) => option.value === "")?.label ?? "请选择"
                          }
                        />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        {[
                          { value: "dismiss", label: "关闭公告" },
                          { value: "open_url", label: "打开链接" },
                        ].map((option) => (
                          <SelectItem
                            key={option.value}
                            value={
                              option.value ||
                              fieldId +
                                "-field-19" +
                                "-" +
                                String(index) +
                                "-" +
                                encodeURIComponent(String("按钮动作")) +
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
                  {button.action === "open_url" && (
                    <Field>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-20" +
                          "-" +
                          String(index) +
                          "-" +
                          encodeURIComponent(String("链接地址"))
                        }
                      >
                        {"链接地址"}
                      </FieldLabel>
                      <Input
                        id={
                          fieldId +
                          "-field-20" +
                          "-" +
                          String(index) +
                          "-" +
                          encodeURIComponent(String("链接地址"))
                        }
                        aria-label={"链接地址"}
                        type="url"
                        required
                        value={String(button.url ?? "")}
                        onChange={(e) => change({ url: e.target.value })}
                      />
                    </Field>
                  )}
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => patch({ buttons: buttons.filter((_, i) => i !== index) })}
                  >
                    移除按钮
                  </Button>
                </div>
              );
            })}
            <Button
              type="button"
              variant="outline"
              onClick={() =>
                patch({ buttons: [...buttons, { text: "", action: "dismiss", url: "" }] })
              }
            >
              添加按钮
            </Button>
          </>
        )}
        <Button
          type="button"
          variant="destructive"
          onClick={() => onChange({ ...root, [key]: null })}
        >
          移除{beacon ? "公告" : "优惠展示"}
        </Button>
      </div>
    );
  }
  if (config.key === "pricing") return <RegionalPricing value={root} onChange={onChange} />;
  return (
    <Alert>
      <Info />
      <AlertDescription>该配置暂无已定义的管理控件。</AlertDescription>
    </Alert>
  );
}
function RegionalPricing({
  value,
  onChange,
}: {
  value: { [key: string]: Json };
  onChange: (next: Json) => void;
}) {
  const fieldId = useId();
  const [country, setCountry] = useState("");
  return (
    <div className="space-y-4">
      {Object.entries(value).map(([code, raw], fieldIndex22) => {
        const row = obj(raw);
        const currency = obj(row.currency_config);
        return (
          <div className="flex flex-wrap items-end gap-2" key={code}>
            <Field>
              <FieldLabel
                htmlFor={
                  fieldId +
                  "-field-24" +
                  "-" +
                  String(fieldIndex22) +
                  "-" +
                  encodeURIComponent(String("地区代码"))
                }
              >
                {"地区代码"}
              </FieldLabel>
              <Input
                id={
                  fieldId +
                  "-field-24" +
                  "-" +
                  String(fieldIndex22) +
                  "-" +
                  encodeURIComponent(String("地区代码"))
                }
                aria-label={"地区代码"}
                readOnly
                value={code}
              />
            </Field>
            <Field>
              <FieldLabel
                htmlFor={
                  fieldId +
                  "-field-25" +
                  "-" +
                  String(fieldIndex22) +
                  "-" +
                  encodeURIComponent(String("货币代码"))
                }
              >
                {"货币代码"}
              </FieldLabel>
              <Input
                id={
                  fieldId +
                  "-field-25" +
                  "-" +
                  String(fieldIndex22) +
                  "-" +
                  encodeURIComponent(String("货币代码"))
                }
                aria-label={"货币代码"}
                required
                maxLength={3}
                value={String(currency.symbol_code ?? "")}
                onChange={(e) =>
                  onChange({
                    ...value,
                    [code]: {
                      ...row,
                      currency_config: { ...currency, symbol_code: e.target.value.toUpperCase() },
                    },
                  })
                }
              />
            </Field>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                const copy = { ...value };
                delete copy[code];
                onChange(copy);
              }}
            >
              移除
            </Button>
          </div>
        );
      })}
      <div className="flex flex-wrap items-end gap-2">
        <Field>
          <FieldLabel
            htmlFor={
              fieldId + "-field-26" + "-" + encodeURIComponent(String("新增地区（两位国家代码）"))
            }
          >
            {"新增地区（两位国家代码）"}
          </FieldLabel>
          <Input
            id={
              fieldId + "-field-26" + "-" + encodeURIComponent(String("新增地区（两位国家代码）"))
            }
            aria-label={"新增地区（两位国家代码）"}
            maxLength={2}
            pattern="[A-Za-z]{2}"
            value={country}
            onChange={(e) => setCountry(e.target.value.toUpperCase())}
            placeholder="US"
          />
        </Field>
        <Button
          variant="outline"
          type="button"
          disabled={!/^[A-Z]{2}$/.test(country) || country in value}
          onClick={() => {
            onChange({
              ...value,
              [country]: {
                country_code: country,
                currency_config: { symbol_code: "", pricing_rollout_gate: null },
              },
            });
            setCountry("");
          }}
        >
          添加地区
        </Button>
      </div>
    </div>
  );
}
export function ClientStatePanel({ id }: { id: string }) {
  const fieldId = useId();
  const [key, setKey] = useState("cloud_preferences");
  const items = accountSections.filter(
    (section) => section.readonly || section.key === "user_settings",
  );
  const selected = items.find((item) => item.key === key) ?? items[0];
  return (
    <div className="min-w-0 space-y-3">
      <Field orientation="horizontal" className="w-fit flex-wrap">
        <FieldLabel htmlFor={`${fieldId}-state`}>状态类别</FieldLabel>
        <Select value={selected?.key} onValueChange={setKey}>
          <SelectTrigger id={`${fieldId}-state`} className="w-64">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {items.map((item) => (
              <SelectItem key={item.key} value={item.key}>
                {item.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      {selected && <ClientState key={`${id}:${selected.key}`} id={id} config={selected} />}
    </div>
  );
}
function ClientState({ id, config }: { id: string; config: (typeof accountSections)[number] }) {
  const resource = useResource<{
    value: Json;
    revision: number | null;
    write_origin: string | null;
    updated_at_ms: number | null;
    fields: BusinessField[];
  }>(`/consumers/${id}/client-state/${config.key}`);
  useErrorToast(resource.error);
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
              <BrowserClientState value={resource.data?.value ?? null} />
            ) : (
              <NamedClientState value={resource.data?.value ?? []} />
            )}
          </>
        }
      </CardContent>
    </Card>
  );
}
const clientLabels: Record<string, string> = {
  title: "标题",
  name: "名称",
  display_name: "显示名称",
  username: "公开用户名",
  email: "邮箱",
  photo_frame_style: "头像形状",
  bio: "简介",
  role: "角色",
  status: "状态",
  selected: "所选语音",
  branch_format: "分支格式",
  git_diff_mode: "差异视图",
  enabled: "启用",
  is_enabled: "已启用",
  approval_mode: "操作审批",
  history_approval_mode: "历史访问审批",
  iab_history_approval_mode: "内置浏览器历史审批",
  download_approval_mode: "下载审批",
  upload_approval_mode: "上传审批",
  disable_auto_review: "关闭自动审查",
  full_cdp_access_enabled: "完整浏览器控制",
  webmcp_enabled: "网页 MCP",
  show_usage_stats_section: "用量统计",
  show_activity_graph_section: "活动图表",
  show_insights_section: "活动分析",
  show_top_plugins_section: "常用插件",
  desktop_onboarding_completed_at: "引导完成时间",
  updated_at: "更新时间",
  created_at: "创建时间",
  id: "记录标识",
  item_id: "资源标识",
  item_type: "资源类型",
  description: "说明",
  instructions: "指令",
  notification_type: "通知类型",
  category: "分类",
  channel: "渠道",
  brand: "品牌",
  last4: "卡号后四位",
  permission: "权限",
  version: "版本",
  contents: "配置正文",
  phone_number: "电话号码",
  relationship: "关系",
  expiry_month: "到期月份",
  expiry_year: "到期年份",
  seen_at: "查看时间",
  reacted_to_at: "处理时间",
  branch_name: "分支",
  repo_url: "仓库地址",
  url: "链接",
  path: "路径",
  localized_interface: "使用客户端所选语言",
  profile_enabled: "显示个人信息页",
  type: "类型",
  message: "内容",
  notification_id: "通知标识",
  clear_cache_only: "仅刷新配置",
  program_id: "活动",
  invite_url: "邀请地址",
  referral_id: "邀请标识",
  can_resend: "可重新发送",
};
function BrowserClientState({ value }: { value: Json }) {
  const root = obj(value);
  const preferences = obj(root.preferences);
  const rules = obj(root.rules);
  const origin = useTablePagination(Object.entries(obj(rules.origin)));
  const download = useTablePagination(Object.entries(obj(rules.download)));
  const upload = useTablePagination(Object.entries(obj(rules.upload)));
  const fullCdp = useTablePagination(Object.entries(obj(rules.full_cdp)));
  const pages = { origin, download, upload, full_cdp: fullCdp };
  return (
    <div className="space-y-4">
      <CardTitle role="heading" aria-level={3}>
        审批偏好
      </CardTitle>
      <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
        {Object.entries(preferences)
          .filter(([key]) => key in clientLabels)
          .map(([key, value]) => ({ label: clientLabels[key], value: clientScalar(value) }))
          .map(({ label, value }) => (
            <Field key={label}>
              <FieldTitle>{label}</FieldTitle>
              <FieldDescription>{value ?? "—"}</FieldDescription>
            </Field>
          ))}
      </FieldGroup>
      {(
        [
          ["origin", "站点访问"],
          ["download", "下载"],
          ["upload", "上传"],
          ["full_cdp", "完整浏览器控制"],
        ] as const
      ).map(([key, title]) => {
        const pagination = pages[key];
        const rows = pagination.rows;
        return (
          <section key={key}>
            <CardTitle role="heading" aria-level={3}>
              {title}规则
            </CardTitle>
            <Table>
              <TableHeader>
                <TableRow>
                  {["站点匹配规则", "审批策略"].map((label) => (
                    <TableHead key={label} scope="col">
                      {label}
                    </TableHead>
                  ))}
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.length ? (
                  <>
                    {rows.map(([site, policy]) => (
                      <TableRow key={site}>
                        <TableCell className="max-w-80 whitespace-normal break-words">
                          {site}
                        </TableCell>
                        <TableCell>{clientScalar(policy)}</TableCell>
                      </TableRow>
                    ))}
                  </>
                ) : (
                  <TableRow>
                    <TableCell colSpan={["站点匹配规则", "审批策略"].length}>
                      <Empty>
                        <EmptyDescription>{"暂无记录"}</EmptyDescription>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
            <Pagination aria-label="记录分页" className="mt-3 justify-end">
              <PaginationContent className="flex-wrap justify-end gap-1">
                <PaginationItem>
                  <Select {...pagination.size}>
                    <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper" side="bottom" align="end">
                      {[10, 20, 30, 50].map((size) => (
                        <SelectItem key={size} value={String(size)}>
                          {size} 条/页
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </PaginationItem>
                <PaginationItem className="mr-2 text-xs text-muted-foreground">
                  共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
                </PaginationItem>
                <PaginationItem>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="首页"
                    {...pagination.first}
                  >
                    <ChevronsLeft />
                  </Button>
                </PaginationItem>
                <PaginationItem>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="上一页"
                    {...pagination.previous}
                  >
                    <ChevronLeft />
                  </Button>
                </PaginationItem>
                <PaginationItem>
                  <Input className="h-7 w-14 text-center tabular-nums" {...pagination.input} />
                </PaginationItem>
                <PaginationItem>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="下一页"
                    {...pagination.next}
                  >
                    <ChevronRight />
                  </Button>
                </PaginationItem>
                <PaginationItem>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="末页"
                    {...pagination.last}
                  >
                    <ChevronsRight />
                  </Button>
                </PaginationItem>
              </PaginationContent>
            </Pagination>
          </section>
        );
      })}
    </div>
  );
}
const clientValues: Record<string, string> = {
  always_ask: "每次询问",
  never_ask: "无需询问",
  disabled: "禁止访问",
  allow: "允许",
  deny: "拒绝",
  unified: "统一视图",
  split: "并排视图",
  circle: "圆形",
  scalloped_circle: "花形",
  rounded_square: "圆角方形",
  oval: "椭圆",
  scalloped_oval: "花边椭圆",
};
function clientScalar(value: Json | undefined) {
  return typeof value === "string" ? (clientValues[value] ?? scalar(value)) : scalar(value);
}
const clientGroups: Record<string, string> = {
  settings: "偏好设置",
  flags: "服务可用性",
  display_settings: "个人页栏目",
  items: "记录列表",
  plugins: "已安装插件",
  gizmo: "项目信息",
  conversations: "项目会话",
  payment_methods: "支付方式",
  members: "成员",
  preferences: "偏好",
  rules: "规则",
  options: "通知渠道",
  payload: "通知内容",
  config_toml: "配置文件",
  requirements_toml: "客户端要求",
  enterprise_managed: "配置条目",
};
function NamedClientState({ value }: { value: Json }) {
  const pagination = useTablePagination(Array.isArray(value) ? value : []);
  if (Array.isArray(value))
    return (
      <>
        <Table>
          <TableHeader>
            <TableRow>
              {["名称 / 内容", "标识", "状态", "详细记录"].map((label) => (
                <TableHead key={label} scope="col">
                  {label}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {value.length ? (
              <>
                {pagination.rows.map((raw, index) => {
                  const row = obj(raw);
                  return (
                    <TableRow key={index}>
                      <TableCell>
                        {typeof raw === "string"
                          ? raw
                          : scalar(
                              row.title ??
                                row.name ??
                                row.email ??
                                row.category ??
                                obj(row.gizmo).name,
                            )}
                      </TableCell>
                      <TableCell className="break-all font-mono text-xs">
                        {scalar(row.id ?? row.item_id ?? obj(row.gizmo).id)}
                      </TableCell>
                      <TableCell>{clientScalar(row.status ?? row.enabled)}</TableCell>
                      <TableCell>
                        {raw && typeof raw === "object" && !Array.isArray(raw) && (
                          <Collapsible>
                            <CollapsibleTrigger asChild>
                              <Button type="button" variant="ghost" size="sm">
                                查看详情
                                <ChevronDown />
                              </Button>
                            </CollapsibleTrigger>
                            <CollapsibleContent>
                              <NamedClientState value={raw} />
                            </CollapsibleContent>
                          </Collapsible>
                        )}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </>
            ) : (
              <TableRow>
                <TableCell colSpan={["名称 / 内容", "标识", "状态", "详细记录"].length}>
                  <Empty>
                    <EmptyDescription>{"暂无记录"}</EmptyDescription>
                  </Empty>
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
        <Pagination aria-label="记录分页" className="mt-3 justify-end">
          <PaginationContent className="flex-wrap justify-end gap-1">
            <PaginationItem>
              <Select {...pagination.size}>
                <SelectTrigger aria-label="每页条数" className="h-7 w-24">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper" side="bottom" align="end">
                  {[10, 20, 30, 50].map((size) => (
                    <SelectItem key={size} value={String(size)}>
                      {size} 条/页
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </PaginationItem>
            <PaginationItem className="mr-2 text-xs text-muted-foreground">
              共 {pagination.total ?? "—"} 条 · {pagination.pages ?? "—"} 页
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="首页"
                {...pagination.first}
              >
                <ChevronsLeft />
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="上一页"
                {...pagination.previous}
              >
                <ChevronLeft />
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Input className="h-7 w-14 text-center tabular-nums" {...pagination.input} />
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="下一页"
                {...pagination.next}
              >
                <ChevronRight />
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label="末页"
                {...pagination.last}
              >
                <ChevronsRight />
              </Button>
            </PaginationItem>
          </PaginationContent>
        </Pagination>
      </>
    );
  if (!value || typeof value !== "object")
    return <CardDescription>{clientScalar(value)}</CardDescription>;
  const entries = Object.entries(value);
  const fields = entries
    .filter(([key, item]) => key in clientLabels && (item === null || typeof item !== "object"))
    .map(([key, item]) => ({ label: clientLabels[key], value: clientScalar(item) }));
  return (
    <div className="space-y-4">
      {fields.length > 0 && (
        <FieldGroup className="grid gap-4 sm:grid-cols-2 gap-3">
          {fields.map(({ label, value }) => (
            <Field key={label}>
              <FieldTitle>{label}</FieldTitle>
              <FieldDescription>{value ?? "—"}</FieldDescription>
            </Field>
          ))}
        </FieldGroup>
      )}
      {entries
        .filter(([, item]) => item !== null && typeof item === "object")
        .map(([key, item]) => (
          <section key={key}>
            {clientGroups[key] && (
              <CardTitle role="heading" aria-level={3}>
                {clientGroups[key]}
              </CardTitle>
            )}
            <NamedClientState value={item} />
          </section>
        ))}
      {!fields.length && !entries.some(([, item]) => item !== null && typeof item === "object") && (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Inbox />
            </EmptyMedia>
            <EmptyDescription>暂无已同步的具名业务记录</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </div>
  );
}
