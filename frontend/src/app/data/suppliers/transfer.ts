"use client";
import { useActions } from "@/lib/actions";
import { request } from "@/lib/api";
import { useState } from "react";
import { toast } from "sonner";

type TransferResultRow = {
  row: number;
  name: string | null;
  status: "imported" | "skipped" | "failed" | "exported";
  message: string;
};
type ImportResult = {
  imported: number;
  skipped: number;
  failed: number;
  results: TransferResultRow[];
};
type ExportData = {
  accounts: { email?: string | null; credentials?: { email?: string | null } }[];
};

export function useSupplierTransfer(mode: "import" | "export", ids: string[], onSaved: () => void) {
  const [format, setFormat] = useState("native");
  const [includeProxies, setIncludeProxies] = useState(false);
  const [file, setFile] = useState<File | null>(null);
  const [result, setResult] = useState<ImportResult>();
  const [exportRows, setExportRows] = useState<TransferResultRow[]>();
  const actions = useActions();
  const busy = actions.isBusy("supplier-transfer");
  const submit = (event: React.SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    void actions.run(
      "supplier-transfer",
      async () => {
        if (mode === "export") {
          if (!ids.length) throw new Error("请选择供应账户");
          const data = await request<ExportData>("/suppliers/export", {
            method: "POST",
            body: { format, account_ids: ids, include_proxies: includeProxies },
          });
          const url = URL.createObjectURL(
            new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }),
          );
          const link = document.createElement("a");
          link.href = url;
          link.download = `${format === "sub2api" ? "sub2api" : "codex2api"}-suppliers-${new Date().toISOString().replaceAll(":", "-")}.json`;
          document.body.appendChild(link);
          link.click();
          link.remove();
          setTimeout(() => URL.revokeObjectURL(url), 1000);
          setExportRows(
            data.accounts.map((account, index) => ({
              row: index + 1,
              name: account.email ?? account.credentials?.email ?? null,
              status: "exported",
              message: "已导出",
            })),
          );
          toast.success("供应账户已导出");
          return;
        }
        if (!file) throw new Error("请选择 JSON 文件");
        if (file.size > 19 * 1024 * 1024) throw new Error("文件不能超过 19 MB");
        let data: unknown;
        try {
          data = JSON.parse(await file.text());
        } catch {
          throw new Error("文件不是有效的 JSON");
        }
        const next = await request<ImportResult>("/suppliers/import", {
          method: "POST",
          body: { format, include_proxies: includeProxies, data },
        });
        setResult(next);
        const message = `导入 ${next.imported} 个，跳过 ${next.skipped} 个，失败 ${next.failed} 个`;
        if (next.failed) toast.error(message);
        else toast.success(message);
        onSaved();
      },
      { success: "" },
    );
  };
  return {
    format,
    setFormat,
    includeProxies,
    setIncludeProxies,
    file,
    selectFile: (file: File | null) => {
      setFile(file);
      setResult(undefined);
    },
    result,
    exportRows,
    resultRows: mode === "export" ? exportRows : result?.results,
    busy,
    submit,
  };
}
