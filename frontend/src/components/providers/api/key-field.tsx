"use client";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Eye, EyeOff } from "lucide-react";
import { useState } from "react";

export function ApiKeyField({
  id,
  value,
  onChange,
  readOnly = false,
}: {
  id: string;
  value: string;
  onChange?: (value: string) => void;
  readOnly?: boolean;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <Field>
      <FieldLabel htmlFor={id}>API Key</FieldLabel>
      <InputGroup>
        <InputGroupInput
          id={id}
          type={visible ? "text" : "password"}
          required={!readOnly}
          readOnly={readOnly}
          autoComplete="off"
          spellCheck={false}
          value={value}
          onChange={(event) => onChange?.(event.target.value)}
        />
        <InputGroupAddon align="inline-end">
          <InputGroupButton
            size="icon-xs"
            aria-label={visible ? "隐藏 API Key" : "显示 API Key"}
            aria-pressed={visible}
            onClick={() => setVisible((current) => !current)}
          >
            {visible ? <EyeOff /> : <Eye />}
          </InputGroupButton>
        </InputGroupAddon>
      </InputGroup>
    </Field>
  );
}
