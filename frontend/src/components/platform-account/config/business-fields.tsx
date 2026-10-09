"use client";
import { Field, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import type { ModelOption } from "@/lib/api";
import { type BusinessField, type Config, type Json } from "@/lib/api";
import { atPath, withPath } from "@/lib/domain";
import { useId } from "react";

export function BusinessFields({
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
  models: ModelOption[];
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
                        <SelectTrigger id={controlId} className="w-full">
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
                        rows={2}
                        maxLength={2000}
                        value={typeof current === "string" ? current : ""}
                        onChange={(event) => update(event.target.value)}
                      />
                    ) : (
                      <Input
                        id={controlId}
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
                  </Field>
                );
              })}
          </div>
        </FieldSet>
      ))}
    </FieldGroup>
  );
}
