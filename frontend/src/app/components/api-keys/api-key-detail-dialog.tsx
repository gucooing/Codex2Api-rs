"use client";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { date } from "@/lib/format";
import { apiCost, apiKeyStatus, apiPlatform, apiTokens, type ApiKey } from "@/lib/metered-api";

export function ApiKeyDetailDialog({
  selected,
  onClose,
}: {
  selected: ApiKey | undefined;
  onClose: () => void;
}) {
  return (
    <Dialog
      open={!!selected}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{selected?.name ?? "API 密钥"}</DialogTitle>
          <DialogDescription>密钥资料与累计用量</DialogDescription>
        </DialogHeader>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-3 text-sm">
          <dt className="text-muted-foreground">用户</dt>
          <dd>{selected?.username ?? "—"}</dd>
          <dt className="text-muted-foreground">密钥前缀</dt>
          <dd>{selected?.key_prefix ?? "—"}…</dd>
          <dt className="text-muted-foreground">API 组</dt>
          <dd>{selected?.group_name ?? "—"}</dd>
          <dt className="text-muted-foreground">平台</dt>
          <dd>{selected ? apiPlatform(selected.provider_id) : "—"}</dd>
          <dt className="text-muted-foreground">状态</dt>
          <dd>{selected ? apiKeyStatus(selected) : "—"}</dd>
          <dt className="text-muted-foreground">创建时间</dt>
          <dd>{date(selected?.created_at)}</dd>
          <dt className="text-muted-foreground">最后使用</dt>
          <dd>{date(selected?.last_used_at)}</dd>
          <dt className="text-muted-foreground">撤销时间</dt>
          <dd>{date(selected?.revoked_at)}</dd>
          <dt className="text-muted-foreground">请求数 / 当前并发</dt>
          <dd>
            {selected?.total_requests ?? "—"} / {selected?.active_requests ?? "—"}
          </dd>
          <dt className="text-muted-foreground">输入 / 输出</dt>
          <dd>
            {selected ? apiTokens(selected.input_tokens) : "—"} /{" "}
            {selected ? apiTokens(selected.output_tokens) : "—"}
          </dd>
          <dt className="text-muted-foreground">计费金额</dt>
          <dd>{selected ? apiCost(selected.cost_nano_usd) : "—"}</dd>
          <dt className="text-muted-foreground">费用未知请求</dt>
          <dd>{selected?.unknown_cost_requests ?? "—"}</dd>
        </dl>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
