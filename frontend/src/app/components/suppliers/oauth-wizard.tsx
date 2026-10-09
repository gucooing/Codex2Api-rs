"use client";
import { useOAuthWizard } from "@/app/data/suppliers";
import { supplierChannel } from "@/components/providers";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardDescription } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { Copy, X } from "lucide-react";
import Link from "next/link";

export function OAuthWizard({
  supplierId,
  supplierProvider = "chatgpt",
  onClose,
  onComplete,
}: {
  supplierId?: string;
  supplierProvider?: string;
  onClose: () => void;
  onComplete: () => void;
}) {
  const {
    dialogFocus,
    fieldId,
    actions,
    provider,
    setProvider,
    channel,
    setup,
    proxies,
    step,
    fingerprint,
    setFingerprint,
    method,
    setMethod,
    refreshToken,
    setRefreshToken,
    batchResults,
    setBatchResults,
    callback,
    setCallback,
    pending,
    busy,
    ready,
    methods,
    steps,
    close,
    back,
    handleSubmit,
    handleClick2,
  } = useOAuthWizard({ supplierId, supplierProvider, onClose, onComplete });
  const ChannelFingerprintFields = supplierChannel(provider).FingerprintFields;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent
        {...dialogFocus}
        showCloseButton={false}
        className="flex max-h-[90dvh] min-h-0 flex-col sm:max-w-3xl"
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (busy) event.preventDefault();
        }}
        onInteractOutside={(event) => {
          if (busy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{supplierId ? "重新授权供应账户" : "添加供应账户"}</DialogTitle>
        </DialogHeader>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="absolute right-4 top-4"
          aria-label="关闭"
          disabled={busy}
          onClick={close}
        >
          <X />
        </Button>
        <Tabs
          value={String(step)}
          onValueChange={(next) => back(Number(next))}
          className="min-h-0 gap-4"
        >
          <TabsList variant="line" className="h-12 w-full shrink-0" aria-label="添加账户步骤">
            {steps.map((item, index) => (
              <TabsTrigger
                key={item.value}
                value={String(item.value)}
                disabled={busy || item.value > step}
                aria-current={item.value === step ? "step" : undefined}
              >
                <Badge variant="outline" className="size-5 justify-center rounded-full p-0">
                  {index + 1}
                </Badge>
                {item.label}
              </TabsTrigger>
            ))}
          </TabsList>
          <form
            noValidate
            className="flex min-h-0 flex-col gap-4"
            aria-busy={busy}
            onSubmit={(event) => handleSubmit(event)}
          >
            <ScrollArea className="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(90dvh-14rem)]">
              <TabsContent value="0" className="space-y-3 pr-1">
                <Field>
                  <FieldLabel htmlFor={`${fieldId}-provider`}>提供商</FieldLabel>
                  <Select
                    value={provider}
                    disabled={busy}
                    onValueChange={(value) => {
                      setProvider(value);
                      setFingerprint(undefined);
                      setBatchResults([]);
                    }}
                  >
                    <SelectTrigger id={`${fieldId}-provider`}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent position="popper">
                      <SelectItem value="chatgpt">ChatGPT</SelectItem>
                      <SelectItem value="grok">Grok</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                {(!setup.ready || !proxies.ready) && (setup.error || proxies.error) && (
                  <Button
                    type="button"
                    variant="outline"
                    disabled={busy}
                    onClick={() => {
                      setup.reload();
                      proxies.reload();
                    }}
                  >
                    重新加载授权配置
                  </Button>
                )}
                <FieldSet disabled={!ready || busy}>
                  <ChannelFingerprintFields
                    value={
                      fingerprint ??
                      setup.data?.fingerprint ?? {
                        os_type: "",
                        os_version: "",
                        arch: "",
                        terminal: "",
                        proxy_id: null,
                        timezone: "",
                      }
                    }
                    onChange={setFingerprint}
                    proxies={proxies.data?.items ?? []}
                  />
                </FieldSet>
              </TabsContent>
              <TabsContent value="1" className="pr-1">
                <RadioGroup
                  value={method}
                  disabled={busy}
                  aria-label="授权方式"
                  onValueChange={(next) => {
                    setMethod(next as typeof method);
                    setRefreshToken("");
                    setBatchResults([]);
                  }}
                >
                  {methods.map((option) => (
                    <FieldLabel key={option.value} htmlFor={`${fieldId}-${option.value}`}>
                      <Field orientation="horizontal">
                        <RadioGroupItem
                          value={option.value}
                          id={`${fieldId}-${option.value}`}
                          aria-label={option.label}
                        />
                        <FieldContent>
                          <FieldTitle>{option.label}</FieldTitle>
                          <FieldDescription>{option.description}</FieldDescription>
                        </FieldContent>
                      </Field>
                    </FieldLabel>
                  ))}
                </RadioGroup>
              </TabsContent>
              <TabsContent value="2" className="space-y-4 pr-1">
                <FieldSet disabled={busy} className="gap-4">
                  {method === "refresh_token" ? (
                    <Field>
                      <FieldLabel htmlFor={`${fieldId}-refresh-token`}>Refresh Token</FieldLabel>
                      <Textarea
                        id={`${fieldId}-refresh-token`}
                        autoComplete="off"
                        spellCheck={false}
                        rows={supplierId ? 2 : 6}
                        required
                        value={refreshToken}
                        onChange={(event) => {
                          setRefreshToken(event.target.value);
                          setBatchResults([]);
                        }}
                      />
                      <FieldDescription>
                        {supplierId ? "输入新的 RT" : "每行一条，最多 50 条"}
                      </FieldDescription>
                    </Field>
                  ) : pending ? (
                    <>
                      {pending.authorize_url && (
                        <Field>
                          <FieldTitle>授权地址</FieldTitle>
                          <div className="flex min-w-0 items-start gap-2">
                            <a
                              id={`${fieldId}-authorize-url`}
                              className="min-w-0 flex-1 break-all text-sm underline underline-offset-4"
                              href={pending.authorize_url}
                              target="_blank"
                              rel="noreferrer"
                            >
                              {pending.authorize_url}
                            </a>
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              disabled={busy || actions.isBusy("copy-supplier-authorize")}
                              onClick={() => handleClick2()}
                            >
                              <Copy />
                              复制
                            </Button>
                          </div>
                        </Field>
                      )}
                      {pending.verification_url && (
                        <Button asChild className="w-fit">
                          <a href={pending.verification_url} target="_blank" rel="noreferrer">
                            打开设备验证页面
                          </a>
                        </Button>
                      )}
                      {pending.user_code && (
                        <Field>
                          <FieldTitle>设备验证码</FieldTitle>
                          <FieldDescription className="select-all font-mono text-xl">
                            {pending.user_code}
                          </FieldDescription>
                        </Field>
                      )}
                      {pending.method === "callback" && (
                        <Field>
                          <FieldLabel htmlFor={`${fieldId}-callback`}>
                            {channel.callbackInputLabel}
                          </FieldLabel>
                          <Textarea
                            id={`${fieldId}-callback`}
                            required
                            rows={3}
                            value={callback}
                            onChange={(event) => setCallback(event.target.value)}
                          />
                        </Field>
                      )}
                    </>
                  ) : (
                    <CardDescription>
                      {busy ? "正在准备授权…" : "点击开始授权，获取授权链接。"}
                    </CardDescription>
                  )}
                </FieldSet>
                {batchResults.length > 0 && (
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>行号</TableHead>
                        <TableHead>结果</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {batchResults.map((row) => (
                        <TableRow key={row.line}>
                          <TableCell>{row.line}</TableCell>
                          <TableCell>
                            {row.status === "running" && <Spinner />}
                            {row.supplierId ? (
                              <Link
                                href={`/suppliers/detail/?id=${encodeURIComponent(row.supplierId)}`}
                              >
                                {row.message}
                              </Link>
                            ) : (
                              row.message
                            )}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                )}
              </TabsContent>
            </ScrollArea>
            <FieldGroup className="flex-row justify-end gap-2 border-t pt-3">
              {batchResults.some((row) => row.status === "complete") && (
                <Button
                  type="button"
                  variant="outline"
                  disabled={busy}
                  onClick={() => {
                    setRefreshToken("");
                    onComplete();
                  }}
                >
                  完成并刷新列表
                </Button>
              )}
              {step > (supplierId ? 1 : 0) && (
                <Button
                  type="button"
                  variant="outline"
                  disabled={busy}
                  onClick={() => back(step - 1)}
                >
                  上一步
                </Button>
              )}
              <Button type="submit" disabled={!ready || busy}>
                {busy && <Spinner />}
                {step < 2
                  ? "下一步"
                  : busy
                    ? "正在提交…"
                    : pending?.method === "callback"
                      ? "提交回调"
                      : pending?.method === "device"
                        ? "检查授权结果"
                        : "开始授权"}
              </Button>
            </FieldGroup>
          </form>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
