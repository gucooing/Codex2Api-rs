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
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { money, type Plan } from "@/lib/api";
import type { Dispatch, SetStateAction } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

type Props = {
  viewedId: string | undefined;
  setViewedId: Dispatch<SetStateAction<string | undefined>>;
  viewed: Plan | undefined;
  purchaseStatus: (plan: Plan) => { disabled: boolean; label: string };
  beginCheckout: (plan: Plan) => void;
};

export function PlanDetailDialog({
  viewedId,
  setViewedId,
  viewed,
  purchaseStatus,
  beginCheckout,
}: Props) {
  return (
    <Dialog
      open={!!viewedId}
      onOpenChange={(open) => {
        if (!open) setViewedId(undefined);
      }}
    >
      <DialogContent className="flex max-h-[90dvh] flex-col sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{viewed?.name ?? "套餐详情"}</DialogTitle>
          <DialogDescription>套餐介绍、订阅权益与可用模型</DialogDescription>
        </DialogHeader>
        <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-12rem)]">
          <div className="space-y-5 pr-3">
            <dl className="grid grid-cols-[auto_1fr_auto_1fr] gap-x-4 gap-y-2 text-sm">
              <dt className="text-muted-foreground">平台</dt>
              <dd>
                {viewed?.provider_id === "chatgpt"
                  ? "ChatGPT"
                  : viewed?.provider_id === "grok"
                    ? "Grok"
                    : (viewed?.provider_id ?? "—")}
              </dd>
              <dt className="text-muted-foreground">售价</dt>
              <dd>
                {viewed?.sale_price_usd == null
                  ? "暂未定价"
                  : `${money(viewed.sale_price_usd)} USD`}
              </dd>
              <dt className="text-muted-foreground">有效期</dt>
              <dd>{viewed ? `${viewed.duration_days} 天` : "—"}</dd>
              <dt className="text-muted-foreground">可用模型</dt>
              <dd>{viewed?.models.length ?? "—"}</dd>
            </dl>
            <section className="space-y-2" aria-label="套餐描述">
              <h3 className="text-sm font-medium">套餐描述</h3>
              <div className="prose prose-sm prose-neutral dark:prose-invert max-w-none break-words [&_table]:block [&_table]:overflow-x-auto">
                <Markdown remarkPlugins={[remarkGfm]} skipHtml>
                  {viewed?.description}
                </Markdown>
              </div>
              {viewed && !viewed.description?.trim() && (
                <p className="text-sm text-muted-foreground">暂无描述</p>
              )}
            </section>
            <section className="space-y-2" aria-label="额度规则">
              <h3 className="text-sm font-medium">额度规则</h3>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>周期</TableHead>
                    <TableHead>额度（USD）</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {viewed?.spending_windows.map((window) => (
                    <TableRow key={window.duration_seconds}>
                      <TableCell>
                        {window.duration_seconds >= 86400
                          ? `${window.duration_seconds / 86400} 天`
                          : `${window.duration_seconds / 3600} 小时`}
                      </TableCell>
                      <TableCell>
                        {window.cost_limit_usd === null ? "不限额" : money(window.cost_limit_usd)}
                      </TableCell>
                    </TableRow>
                  ))}
                  {viewed?.spending_windows.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={2}>不限额</TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
              {(viewed?.spending_windows.length ?? 0) > 1 && (
                <p className="text-xs text-muted-foreground">
                  各周期额度同时生效，短周期用量也计入长周期。
                </p>
              )}
            </section>
            <section className="space-y-2" aria-label="可用模型">
              <h3 className="text-sm font-medium">可用模型</h3>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>模型</TableHead>
                    <TableHead>类型</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {viewed?.models.map((model) => (
                    <TableRow key={`${model.provider_id}/${model.model}`}>
                      <TableCell className="break-all whitespace-normal">{model.model}</TableCell>
                      <TableCell>
                        {model.kind === "text"
                          ? "文本"
                          : model.kind === "image"
                            ? "图像"
                            : model.kind}
                      </TableCell>
                    </TableRow>
                  ))}
                  {viewed?.models.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={2}>暂无可用模型</TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            </section>
          </div>
        </ScrollArea>
        <DialogFooter>
          <Button variant="outline" onClick={() => setViewedId(undefined)}>
            关闭
          </Button>
          <Button
            disabled={!viewed || purchaseStatus(viewed).disabled}
            onClick={() => {
              if (viewed) beginCheckout(viewed);
            }}
          >
            {viewed ? purchaseStatus(viewed).label : "下单"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
