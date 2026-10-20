"use client";
import { Empty, EmptyDescription } from "@/components/ui/empty";

/** The API-key channel has no official quota or profile read. */
export function ApiOfficialData({
  section,
}: {
  id: string;
  section: string;
  onUsername?: (username: string | undefined) => void;
  onUpdated?: () => void;
}) {
  return (
    <Empty>
      <EmptyDescription>
        API 渠道不提供官方{section === "quota" ? "额度" : "数据"}；本地用量见“本地用量”页签
      </EmptyDescription>
    </Empty>
  );
}
