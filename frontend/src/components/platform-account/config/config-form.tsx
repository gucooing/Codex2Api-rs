"use client";
import { BusinessFields } from "@/components/platform-account/config/business-fields";
import { ComplexService } from "@/components/platform-account/config/complex-service";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  Field,
  FieldContent,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { type Config } from "@/lib/api";
import { atPath, withPath } from "@/lib/domain";
import { useConfigForm } from "@/lib/platform-account/config";
import { ChevronDown, RotateCcw } from "lucide-react";

export function ConfigForm({
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
  const {
    fieldId,
    busy,
    drafts,
    setDrafts,
    configs,
    models,
    valueOf,
    change,
    pending,
    switches,
    fields,
    handleSubmit,
  } = useConfigForm({ id, group, items, disabled, onSaved });
  const additionalFields = fields.map((config) => (
    <BusinessFields
      key={config.key}
      config={config}
      fields={config.fields.filter((field) => field.kind !== "boolean")}
      value={valueOf(config)}
      onChange={(value) => change(config, value)}
      models={models.data?.items ?? []}
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
      </CardHeader>
      <CardContent>
        <form
          id={`${fieldId}-form`}
          noValidate
          aria-busy={busy}
          onSubmit={(event) => handleSubmit(event)}
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
                      </FieldContent>
                      <div className="flex shrink-0 items-center gap-2">
                        {current == null && <Badge variant="outline">默认</Badge>}
                        <Switch
                          id={controlId}
                          checked={current === true}
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
