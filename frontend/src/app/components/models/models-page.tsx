"use client";
import { ModelPricing } from "@/app/components/models/model-pricing";
import { SearchBilling } from "@/components/providers/chatgpt/billing";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export default function BillingPage() {
  return (
    <Tabs defaultValue="models">
      <TabsList>
        <TabsTrigger value="models">模型计费</TabsTrigger>
        <TabsTrigger value="requests">其他计费</TabsTrigger>
      </TabsList>
      <TabsContent value="models" className="space-y-3">
        <ModelPricing />
      </TabsContent>
      <TabsContent value="requests">
        <SearchBilling />
      </TabsContent>
    </Tabs>
  );
}
