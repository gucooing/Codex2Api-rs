"use client";
import { Desktop } from "@/app/components/settings/desktop";
import { Gateway } from "@/app/components/settings/gateway";
import { PublicUrls } from "@/app/components/settings/public-urls";
import { ResourceTable } from "@/app/components/settings/resource-table";
import { Security } from "@/app/components/settings/security";
import { useSettingsPage } from "@/app/data/settings";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

const settingTabs = [
  ["gateway", "网关与限流"],
  ["public-urls", "访问地址"],
  ["security", "管理员凭据"],
  ["desktop", "Desktop 支持"],
  ["resources", "公开资源"],
  ["missing", "端点诊断"],
] as const;

export default function SettingsPage() {
  const { tab, setTab } = useSettingsPage();
  return (
    <>
      <Tabs value={tab} onValueChange={setTab} className="min-w-0 gap-4">
        <div className="max-w-full overflow-x-auto overflow-y-hidden pb-1">
          <TabsList variant="line" aria-label={"系统设置"}>
            {settingTabs.map(([key, text]) => (
              <TabsTrigger key={key} value={key}>
                {text}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        <TabsContent value={tab} className="space-y-4">
          {tab === "gateway" && <Gateway />}
          {tab === "public-urls" && <PublicUrls />}
          {tab === "security" && <Security />}
          {tab === "desktop" && <Desktop />}
          {tab === "resources" && (
            <ResourceTable
              path="/resources"
              title="公开资源缓存"
              columns={["资源", "大小", "缓存时间"]}
              fields={[["path"], ["bytes"], ["fetched_at_ms", "date"]]}
            />
          )}
          {tab === "missing" && (
            <ResourceTable
              path="/missing-endpoints"
              title="缺失端点记录"
              columns={["方法", "接口", "请求次数", "最近请求"]}
              fields={[["method"], ["path"], ["hits"], ["last_seen_at", "date"]]}
            />
          )}
        </TabsContent>
      </Tabs>
    </>
  );
}
