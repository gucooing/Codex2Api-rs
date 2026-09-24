import type { Json } from "./api";

function object(value: Json | undefined): Record<string, Json> {
  return value && typeof value === "object" && !Array.isArray(value) ? value : {};
}
export function resourceRecord(row: Json) {
  const envelope = object(row);
  return {
    id: envelope.id,
    owner: envelope.owner,
    source: envelope.source,
    created_at_ms: envelope.created_at_ms,
    updated_at_ms: envelope.updated_at_ms,
    value: object(envelope.value),
  };
}
export function taskRecord(row: Json) {
  const envelope = resourceRecord(row);
  const task = Object.hasOwn(envelope.value, "task") ? object(envelope.value.task) : envelope.value;
  const assistant = object(envelope.value.current_assistant_turn);
  const display = object(object(task.task_status_display).latest_turn_status_display);
  return {
    title: task.title,
    id: task.id,
    status: assistant.turn_status ?? display.turn_status ?? task.status,
    source: envelope.source,
    created_at_ms: envelope.created_at_ms,
    updated_at_ms: envelope.updated_at_ms,
  };
}
export function subscriptionRecord(row: Json) {
  const envelope = resourceRecord(row);
  return {
    id: envelope.id,
    operation: envelope.value.operation,
    origin: envelope.value.origin,
    plan_name: envelope.value.plan_name,
    previous_plan_name: envelope.value.previous_plan_name,
    expires_at: envelope.value.expires_at,
    created_at_ms: envelope.value.created_at_ms,
  };
}
