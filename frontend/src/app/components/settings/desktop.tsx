"use client";
import { useDesktop } from "@/app/data/settings";
import { Button } from "@/components/ui/button";
import { Field, FieldGroup, FieldLabel, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { CircleAlert } from "lucide-react";

export function Desktop() {
  const {
    fieldId,
    resource,
    proxies,
    busy,
    disabled,
    proxyValue,
    cacheMinutes,
    selectProxy,
    setCacheMinutes,
    handleSubmit,
  } = useDesktop();
  return (
    <form
      noValidate
      className="space-y-4"
      aria-busy={busy || resource.refreshing || proxies.refreshing}
      onSubmit={handleSubmit}
    >
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => {
            resource.reload();
            proxies.reload();
          }}
          disabled={resource.refreshing || proxies.refreshing || busy}
        >
          {(resource.refreshing || proxies.refreshing) && <Spinner />}刷新
        </Button>
        <Button type="submit" size="sm" disabled={disabled}>
          {busy && <Spinner />}
          {busy ? "正在保存…" : "保存"}
        </Button>
      </div>
      <FieldSet>
        <FieldGroup className="grid gap-4 sm:grid-cols-2">
          <Field>
            <div className="flex items-center gap-1">
              <FieldLabel htmlFor={`${fieldId}-proxy`}>公开资源出站代理</FieldLabel>
              <Popover>
                <PopoverTrigger asChild>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    className="size-5 text-muted-foreground"
                    aria-label="公开资源出站代理说明"
                  >
                    <CircleAlert className="size-3" aria-hidden="true" />
                  </Button>
                </PopoverTrigger>
                <PopoverContent
                  align="start"
                  collisionPadding={16}
                  aria-label="公开资源出站代理说明"
                >
                  服务器下载 Desktop
                  公开资源时使用哪个网络代理。选择“使用服务器默认网络”就沿用服务器的网络设置。这项只控制公开资源的下载。
                </PopoverContent>
              </Popover>
            </div>
            <Select value={proxyValue} onValueChange={selectProxy} disabled={disabled}>
              <SelectTrigger id={`${fieldId}-proxy`} className="w-full">
                <SelectValue placeholder="尚未加载" />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="default">使用服务器默认网络</SelectItem>
                {(proxies.data?.items ?? []).map((proxy) => (
                  <SelectItem key={proxy.id} value={proxy.id}>
                    {proxy.name} · {proxy.display_url}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <div className="flex items-center gap-1">
              <FieldLabel htmlFor={`${fieldId}-cache`}>公开资源缓存时间（分钟）</FieldLabel>
              <Popover>
                <PopoverTrigger asChild>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    className="size-5 text-muted-foreground"
                    aria-label="公开资源缓存时间（分钟）说明"
                  >
                    <CircleAlert className="size-3" aria-hidden="true" />
                  </Button>
                </PopoverTrigger>
                <PopoverContent
                  align="start"
                  collisionPadding={16}
                  aria-label="公开资源缓存时间（分钟）说明"
                >
                  下载过的公开资源可以重复使用多久，减少重复下载。到期后，再次读取时会获取新内容。可填
                  1 到 1440 分钟，1440 分钟就是 1 天。
                </PopoverContent>
              </Popover>
            </div>
            <Input
              id={`${fieldId}-cache`}
              type="number"
              required
              min={1}
              max={1440}
              step={1}
              value={cacheMinutes}
              onChange={(event) => setCacheMinutes(event.target.value)}
              disabled={disabled}
            />
          </Field>
        </FieldGroup>
      </FieldSet>
    </form>
  );
}
