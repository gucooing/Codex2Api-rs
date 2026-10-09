"use client";
import { ClientStatePanel } from "@/components/platform-account/client-state/client-state-panel";
import { Logs } from "@/components/platform-account/logs";
import { Records } from "@/components/platform-account/records";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useClientRecords } from "@/lib/platform-account";

export function ClientRecords({ id }: { id: string }) {
  const { fieldId, kind, setKind, kinds } = useClientRecords();
  return (
    <>
      <div className="flex flex-wrap items-center gap-3">
        <Field orientation="horizontal" className="w-fit flex-wrap">
          <FieldLabel htmlFor={`${fieldId}-kind`}>记录类别</FieldLabel>
          <Select value={kind} onValueChange={setKind}>
            <SelectTrigger id={`${fieldId}-kind`} className="w-64">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              {kinds.map(([key, label]) => (
                <SelectItem key={key} value={key}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
      </div>
      {kind === "logs" ? (
        <Logs id={id} />
      ) : kind === "state" ? (
        <ClientStatePanel id={id} />
      ) : (
        <Records
          key={kind}
          id={id}
          kind={kind}
          title={kinds.find(([key]) => key === kind)?.[1] ?? "记录"}
        />
      )}
    </>
  );
}
