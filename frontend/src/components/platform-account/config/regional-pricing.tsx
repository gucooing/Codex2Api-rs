"use client";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type Json } from "@/lib/api";
import { obj, useRegionalPricing } from "@/lib/platform-account/config";

export function RegionalPricing({
  value,
  onChange,
}: {
  value: { [key: string]: Json };
  onChange: (next: Json) => void;
}) {
  const { fieldId, country, setCountry } = useRegionalPricing();
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
