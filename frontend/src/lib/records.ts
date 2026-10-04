import { date } from "@/lib/format";
import type { Json } from "./api";
import { taskRecord, subscriptionRecord } from "./record-selectors";
import { atPath } from "./domain";

type Column = readonly [string, readonly string[], "date"?];

/** Compact columns are frontend business choices; full fields remain in record details. */
export function mobileRecordColumns(kind: string): string[] {
  const preferred: Record<string, string[]> = {
    logs: ["接口", "状态"],
    analytics: ["活动", "接收时间"],
    site_status: ["网站", "代理访问限制"],
    remote_servers: ["主机", "系统"],
    task: ["任务", "状态"],
    conversation: ["会话", "状态"],
    task_operation: ["任务", "状态"],
    subscription_operation: ["操作", "套餐"],
    connector_catalog: ["连接器", "状态"],
    family_notices: ["成员", "状态"],
  };
  return preferred[kind] ?? recordColumns(kind).slice(0, 2);
}
export function diagnosticSummary(value: unknown): string {
  if (!Array.isArray(value)) return "—";
  return (
    value
      .slice(0, 8)
      .map((item) => {
        if (!item || typeof item !== "object") return "";
        if (typeof item.metric === "string") {
          const name = [item.namespace, item.metric].filter(Boolean).join(".");
          return Array.isArray(item.values)
            ? `${name}：${item.values.length} 个样本`
            : `${name}：${item.value}`;
        }
        return [item.event, item.exception, item.reason]
          .filter((field) => typeof field === "string")
          .join(" / ");
      })
      .filter(Boolean)
      .join("；") || "—"
  );
}
const columns: Record<string, readonly Column[]> = {
  cloud_environment: [
    ["环境", ["label"]],
    ["标识", ["id"]],
    ["仓库", ["repos"]],
  ],
  realtime_call: [
    ["通话标识", ["id"]],
    ["模型", ["model"]],
    ["转录模型", ["transcription_model"]],
    ["创建结果", ["status"]],
    ["供应账户", ["source"]],
    ["创建时间", ["created_at_ms"], "date"],
  ],
  task_execution: [
    ["任务", ["task_id"]],
    ["模型", ["model"]],
    ["供应账户", ["source"]],
    ["执行记录时间", ["created_at_ms"], "date"],
  ],
  task_operation: [
    ["任务", ["task_id"]],
    ["操作", ["operation"]],
    ["状态", ["status"]],
    ["失败原因", ["reason"]],
    ["时间", ["created_at_ms"], "date"],
  ],
  logs: [
    ["时间", ["started_at_ms", "created_at_ms", "requested_at_ms"], "date"],
    ["方法", ["method"]],
    ["接口", ["path", "endpoint"]],
    ["状态", ["status", "status_code"]],
    ["耗时（毫秒）", ["duration_ms", "elapsed_ms"]],
  ],
  analytics: [
    ["接收时间", ["received_at_ms"], "date"],
    ["活动", ["event_type"]],
    ["详情", ["action", "rating", "status"]],
    ["模型", ["model", "model_slug"]],
    ["插件 / 技能", ["plugin_name", "skill_name"]],
    ["会话", ["thread_id"]],
    ["轮次", ["turn_id"]],
  ],
  site_status: [
    ["网站", ["host"]],
    ["代理访问限制", ["feature_status.agent"]],
    ["页面访问限制", ["feature_status.page_content"]],
    ["查询时间", ["checked_at"], "date"],
  ],
  remote_servers: [
    ["主机", ["name"]],
    ["环境", ["environment_id"]],
    ["系统", ["os"]],
    ["架构", ["arch"]],
    ["客户端版本", ["app_server_version"]],
    ["最近活动", ["last_seen_at_ms"], "date"],
    ["连接有效期", ["connected_until_ms"], "date"],
  ],
  subscription_operation: [
    ["操作", ["operation", "action", "kind"]],
    ["时间", ["created_at_ms", "at_ms"], "date"],
    ["套餐", ["plan_name", "plan_id", "after.plan_id"]],
    [
      "订阅期限",
      ["expires_at", "subscription_expires_at", "after.subscription_expires_at"],
      "date",
    ],
    ["操作者", ["origin", "actor"]],
  ],
  task: [
    ["任务", ["title", "name", "id"]],
    ["状态", ["status"]],
    ["供应账户", ["source", "supplier_account_id", "account_id"]],
    ["模型", ["model"]],
    ["创建时间", ["created_at_ms", "created_at"], "date"],
    ["更新时间", ["updated_at_ms", "updated_at"], "date"],
  ],
  conversation: [
    ["会话", ["title", "name"]],
    ["标识", ["id", "conversation_id"]],
    ["状态", ["status"]],
    ["创建时间", ["created_at_ms", "created_at", "create_time"], "date"],
    ["更新时间", ["updated_at_ms", "updated_at", "update_time"], "date"],
  ],
  connector_catalog: [
    ["连接器", ["name", "title"]],
    ["标识", ["id"]],
    ["说明", ["description"]],
    ["状态", ["status"]],
  ],
  family_notices: [
    ["成员", ["name", "member_name", "email"]],
    ["状态", ["status"]],
    ["创建时间", ["created_at_ms", "created_at"], "date"],
    ["处理时间", ["reacted_to_at"], "date"],
  ],
  diagnostics: [
    ["最近接收", ["last_seen_at_ms"], "date"],
    ["来源", ["source"]],
    ["账户", ["owner"]],
    ["事件数", ["record_count"]],
    ["接收次数", ["attempts"]],
  ],
  resources: [
    ["资源路径", ["path"]],
    ["字节数", ["bytes"]],
    ["缓存时间", ["fetched_at_ms"], "date"],
  ],
  missing: [
    ["方法", ["method"]],
    ["接口", ["path"]],
    ["次数", ["count", "hits"]],
    ["最近请求", ["last_seen_at_ms", "last_seen_at"], "date"],
  ],
};
const operationColumns: readonly Column[] = [
  ["操作", ["operation", "action", "method"]],
  ["资源", ["task_id", "resource_id", "id"]],
  ["状态", ["status", "capture_status"]],
  ["供应账户", ["source", "supplier_account_id", "account_id"]],
  ["记录时间", ["updated_at_ms", "created_at_ms", "received_at_ms"], "date"],
];
const statusLabels: Record<string, string> = {
  created: "已创建",
  completed: "已完成",
  finished_successfully: "已完成",
  in_progress: "进行中",
  running: "进行中",
  pending: "等待中",
  queued: "等待中",
  failed: "失败",
  interrupted: "中断",
  cancelled: "已取消",
  unknown: "未提供",
  granted: "发放",
  renewed: "续期",
  changed: "调整",
  expired: "到期",
  grant: "发放",
  reissue: "重新发放",
  admin_reissue: "管理员重新发放",
  order: "订单支付",
  order_upgrade: "订单升级",
  renew: "续期",
  change_plan: "调整套餐",
  change_expiry: "调整期限",
  enable: "启用",
  disable: "停用",
  wallet: "钱包购买",
  free: "Free",
};
function object(value: Json): Record<string, Json> {
  return value && typeof value === "object" && !Array.isArray(value) ? value : {};
}
export function unwrapRecord(row: Json) {
  const item = object(row);
  const value = object(item.value ?? null);
  return { ...item, ...value };
}
export function scalar(value: Json | undefined): string {
  if (value == null) return "—";
  if (typeof value === "boolean") return value ? "是" : "否";
  if (typeof value === "object") return Array.isArray(value) ? `${value.length} 条记录` : "已保存";
  return statusLabels[String(value)] ?? String(value);
}
export function recordColumns(kind: string) {
  const fields: readonly Column[] =
    kind === "task"
      ? [
          ["任务", ["title"]],
          ["状态", ["status"]],
          ["供应账户", ["source"]],
          ["创建时间", ["created_at_ms"], "date"],
          ["更新时间", ["updated_at_ms"], "date"],
        ]
      : kind === "subscription_operation"
        ? [
            ["操作", ["operation"]],
            ["套餐", ["plan_name"]],
            ["先前套餐", ["previous_plan_name"]],
            ["订阅期限", ["expires_at"], "date"],
            ["来源", ["origin"]],
            ["时间", ["created_at_ms"], "date"],
          ]
        : (columns[kind] ?? operationColumns);
  return fields.map(([label]) => label);
}
export function recordRows(kind: string, rows: Json[]) {
  const fields: readonly Column[] =
    kind === "task"
      ? [
          ["任务", ["title"]],
          ["状态", ["status"]],
          ["供应账户", ["source"]],
          ["创建时间", ["created_at_ms"], "date"],
          ["更新时间", ["updated_at_ms"], "date"],
        ]
      : kind === "subscription_operation"
        ? [
            ["操作", ["operation"]],
            ["套餐", ["plan_name"]],
            ["先前套餐", ["previous_plan_name"]],
            ["订阅期限", ["expires_at"], "date"],
            ["来源", ["origin"]],
            ["时间", ["created_at_ms"], "date"],
          ]
        : (columns[kind] ?? operationColumns);
  return rows.map((row, index) => {
    const item =
      kind === "task"
        ? taskRecord(row)
        : kind === "subscription_operation"
          ? subscriptionRecord(row)
          : unwrapRecord(row);
    return {
      key: typeof item.id === "string" || typeof item.id === "number" ? item.id : index,
      cells: fields.map(([label, paths, format]) => {
        const value = paths
          .map((path) => atPath(item, path.split(".")))
          .find((value) => value != null);
        return {
          label,
          text:
            format === "date"
              ? date(typeof value === "number" || typeof value === "string" ? value : null)
              : scalar(value),
          status: label === "状态" && value != null,
          failed:
            typeof value === "number"
              ? value >= 400
              : ["failed", "interrupted"].includes(String(value)),
        };
      }),
    };
  });
}
