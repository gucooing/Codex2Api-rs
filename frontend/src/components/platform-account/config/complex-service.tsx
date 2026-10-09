"use client";
import { RegionalPricing } from "@/components/platform-account/config/regional-pricing";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { type Config, type Json } from "@/lib/api";
import { obj, useComplexService } from "@/lib/platform-account/config";
import { Info } from "lucide-react";

export function ComplexService({
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
  const { fieldId, plugins, connectors, root } = useComplexService({ config, id, value });
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
                          data-required="true"
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
