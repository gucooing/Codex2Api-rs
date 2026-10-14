"use client";
import { RoutingForm } from "@/components/forms/platform-routing-form";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { type SubscriptionAccount } from "@/lib/api";
import { useRouting } from "@/lib/platform-account";

export function Routing({ account, id }: { account?: SubscriptionAccount; id: string }) {
  const { resource } = useRouting({ id });
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle role="heading" aria-level={2}>
          供应号池
        </CardTitle>
      </CardHeader>
      <CardContent>
        {
          <RoutingForm
            key={id}
            id={id}
            account={account}
            data={resource.data ?? { items: [] }}
            disabled={!resource.ready || !account}
            onSaved={resource.reload}
          />
        }
      </CardContent>
    </Card>
  );
}
