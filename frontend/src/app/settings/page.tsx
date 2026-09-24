"use client";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { useTablePagination, usePageControls } from "@/lib/pagination";
import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";

import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Spinner } from "@/components/ui/spinner";
import { FieldSet, FieldGroup, Field, FieldLabel, FieldDescription } from "@/components/ui/field";
import { Button } from "@/components/ui/button";
import { useId } from "react";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { useActions, useErrorToast } from "@/lib/actions";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
} from "@/components/ui/table";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import { date } from "@/lib/format";
import { useState } from "react";
import { useRouter } from "next/navigation";
import {
  request,
  type DesktopSettings,
  type GatewaySettings,
  type List,
  type Proxy,
} from "@/lib/api";
import { useResource } from "@/lib/hooks";

const settingTabs = [
  ["gateway", "网关 UA"],
  ["security", "管理员凭据"],
  ["desktop", "Desktop 支持"],
  ["diagnostics", "诊断记录"],
  ["resources", "公开资源"],
  ["missing", "端点诊断"],
] as const;
export default function SettingsPage() {
  const [tab, setTab] = useState("gateway");
  return (
    <>
      <Tabs value={tab} onValueChange={setTab} className="min-w-0 gap-4">
        <div className="max-w-full overflow-x-auto overflow-y-hidden pb-1">
          <TabsList variant="line" aria-label={"系统设置"}>
            {settingTabs.map(([key, text]) => (
              <TabsTrigger key={key} value={key}>
                {text}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        <TabsContent value={tab} className="space-y-4">
          {tab === "gateway" && <Gateway />}
          {tab === "security" && <Security />}
          {tab === "desktop" && <Desktop />}
          {tab === "diagnostics" && (
            <DiagnosticTable
              path="/diagnostics"
              title="客户端诊断记录"
              columns={["最近接收", "来源", "归属", "事件数", "接收次数"]}
              fields={[
                ["last_seen_at_ms", "date"],
                ["source"],
                ["owner"],
                ["record_count"],
                ["attempts"],
              ]}
            />
          )}
          {tab === "resources" && (
            <DiagnosticTable
              path="/resources"
              title="公开资源缓存"
              columns={["资源", "大小", "缓存时间"]}
              fields={[["path"], ["bytes"], ["fetched_at_ms", "date"]]}
            />
          )}
          {tab === "missing" && (
            <DiagnosticTable
              path="/missing-endpoints"
              title="缺失端点记录"
              columns={["方法", "接口", "请求次数", "最近请求"]}
              fields={[["method"], ["path"], ["hits"], ["last_seen_at", "date"]]}
            />
          )}
        </TabsContent>
      </Tabs>
    </>
  );
}
function Gateway() {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<GatewaySettings>("/settings/gateway");
  const [value, setValue] = useState<GatewaySettings>();
  const [rulesText, setRulesText] = useState<string>();
  useErrorToast(resource.error ? resource.error : undefined);
  const current = value ?? resource.data ?? { ua_mode: "blacklist", ua_rules: [] };
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {"网关 UA 规则"}
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
        <CardDescription className="text-sm text-muted-foreground">
          规则应用于虚拟账户执行接口和 WebSocket 入口。黑名单命中时拒绝请求，白名单仅允许命中的 UA。
        </CardDescription>
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("app\\settings\\page.tsx:form:1")}
          onSubmit={(event) =>
            actions.submit(
              event,
              "app\\settings\\page.tsx:form:1",
              async () => {
                if (!resource.ready) throw new Error("请先加载设置");
                await request("/settings/gateway", {
                  method: "PUT",
                  body: {
                    ...current,
                    ua_rules: (rulesText ?? current.ua_rules.join("\n"))
                      .split(/\r?\n/)
                      .map((item) => item.trim())
                      .filter(Boolean),
                  },
                });
              },
              "网关设置已保存",
            )
          }
        >
          <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
            <FieldSet
              disabled={!resource.ready || actions.isBusy("app\\settings\\page.tsx:form:1")}
              className="min-h-0 overflow-y-auto pr-1"
            >
              <FieldGroup className="gap-4">
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      访问策略
                    </CardTitle>
                    <CardDescription>按客户端的 User-Agent 请求头匹配。</CardDescription>
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
                        data-required={false ? "true" : undefined}
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
function Security() {
  const resource = useResource<{ username: string }>("/settings/security");
  useErrorToast(resource.error);
  return (
    <>
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={resource.reload}
        disabled={resource.refreshing}
      >
        重新加载
      </Button>
      <SecurityEditor
        key={resource.data?.username ?? "unloaded"}
        username={resource.data?.username ?? ""}
        disabled={!resource.ready}
      />
    </>
  );
}
function SecurityEditor({ username, disabled }: { username: string; disabled: boolean }) {
  const fieldId = useId();
  const actions = useActions();
  const router = useRouter();
  const [form, setForm] = useState({
    old_username: username,
    old_password: "",
    new_username: username,
    new_password: "",
  });
  const update = (key: keyof typeof form, next: string) =>
    setForm((value) => ({ ...value, [key]: next }));
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
          onSubmit={(event) =>
            actions.submit(
              event,
              "app\\settings\\page.tsx:form:4",
              async () => {
                if (disabled) throw new Error("请先加载设置");
                await request("/settings/security", { method: "PUT", body: form });
                window.dispatchEvent(new Event("admin-session-expired"));
                router.replace("/");
              },
              "已保存",
            )
          }
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
function Desktop() {
  const fieldId = useId();
  const actions = useActions();
  const resource = useResource<DesktopSettings>("/settings/desktop");
  const proxies = useResource<List<Proxy>>("/proxies");
  const [value, setValue] = useState<DesktopSettings>();
  useErrorToast(resource.error ? resource.error : undefined);
  useErrorToast(proxies.error);
  const current = value ??
    resource.data ?? { proxy_id: null, resource_cache_minutes: 0, collect_diagnostics: false };
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
        <CardDescription className="text-sm text-muted-foreground">
          公开资源缓存、客户端诊断元数据与资源出站代理。客户端更新状态由官方服务提供。
        </CardDescription>
        <form
          noValidate
          className="flex min-h-0 flex-col gap-4"
          aria-busy={actions.isBusy("app\\settings\\page.tsx:form:9")}
          onSubmit={(event) =>
            actions.submit(
              event,
              "app\\settings\\page.tsx:form:9",
              async () => {
                if (!resource.ready) throw new Error("请先加载设置");
                await request("/settings/desktop", { method: "PUT", body: current });
              },
              "Desktop 支持设置已保存",
            )
          }
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
                          data-required={false ? "true" : undefined}
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
                <section className="space-y-3">
                  <div className="space-y-1">
                    <CardTitle role="heading" aria-level={3}>
                      诊断采集
                    </CardTitle>
                  </div>
                  <Field orientation="horizontal">
                    <Switch
                      id={
                        fieldId +
                        "-field-12" +
                        "-" +
                        encodeURIComponent(String("保留客户端诊断元数据"))
                      }
                      checked={current.collect_diagnostics}
                      onCheckedChange={(collect_diagnostics) =>
                        setValue({ ...current, collect_diagnostics })
                      }
                    />
                    <div>
                      <FieldLabel
                        htmlFor={
                          fieldId +
                          "-field-12" +
                          "-" +
                          encodeURIComponent(String("保留客户端诊断元数据"))
                        }
                      >
                        {"保留客户端诊断元数据"}
                      </FieldLabel>
                      <FieldDescription>{"接收的诊断记录可在“诊断记录”中查看。"}</FieldDescription>
                    </div>
                  </Field>
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
function DiagnosticTable({
  path,
  title,
  columns,
  fields,
}: {
  path: string;
  title: string;
  columns: string[];
  fields: [string, "date"?][];
}) {
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const resource = useResource<{
    items: Record<string, unknown>[];
    total?: number;
    page?: number;
    page_size?: number;
  }>(path === "/diagnostics" ? `${path}?page=${page}&page_size=${pageSize}` : path);
  useErrorToast(resource.error);
  const local = useTablePagination(resource.data?.items ?? [], path, resource.data !== undefined);
  const remote = usePageControls(
    resource.data?.page ?? page,
    resource.data?.total,
    setPage,
    pageSize,
    resource.refreshing,
    setPageSize,
  );
  const pagination = path === "/diagnostics" ? remote : local;
  const items = path === "/diagnostics" ? (resource.data?.items ?? []) : local.rows;
  return (
    <Card>
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          {title}
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
            重新加载
          </Button>
        )}
        <Table>
          <TableHeader>
            <TableRow>
              {columns.map((label) => (
                <TableHead key={label} scope="col">
                  {label}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {items.length ? (
              <>
                {items.map((item, index) => (
                  <TableRow key={String(item.id ?? index)}>
                    {fields.map(([field, format]) => (
                      <TableCell key={field} className="max-w-80 whitespace-normal break-words">
                        {format === "date"
                          ? date(
                              typeof item[field] === "number" || typeof item[field] === "string"
                                ? (item[field] as string | number)
                                : null,
                            )
                          : item[field] == null
                            ? "—"
                            : String(item[field])}
                      </TableCell>
                    ))}
                  </TableRow>
                ))}
              </>
            ) : (
              <TableRow>
                <TableCell colSpan={columns.length}>
                  <Empty>
                    <EmptyDescription>
                      {resource.loading
                        ? "正在加载…"
                        : resource.error
                          ? "尚未取得记录"
                          : "暂无记录"}
                    </EmptyDescription>
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
      </CardContent>
    </Card>
  );
}
