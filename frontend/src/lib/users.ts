export type User = {
  id: string;
  username: string;
  name: string;
  email: string;
  enabled: boolean;
  wallet_balance_usd: string;
  revision: number;
  created_at: string;
};
export type UserSubscription = {
  expired: boolean;
  user_id: string;
  username: string;
  name: string;
  virtual_account_id: string;
  provider_id: string;
  plan_id: string;
  plan_name: string;
  plan_type: string;
  subscription_expires_at: string | null;
  enabled: boolean;
  revision: number;
  created_at: string;
};
export type WalletEntry = {
  id: string;
  kind: "order_payment" | "system_adjustment";
  balance_before_cents: number;
  operator_name: string | null;
  reason: string | null;
  plan_name: string | null;
  amount_cents: number;
  balance_cents: number;
  created_at: string;
  expires_at: string | null;
  order_id: string | null;
};
export type AdminWalletEntry = WalletEntry & {
  user_id: string;
  username: string;
  user_name: string;
};
export type WalletEntryPage = {
  items: AdminWalletEntry[];
  total: number;
  page: number;
  limit: number;
};
export type UserDetail = {
  user: User;
  subscriptions: UserSubscription[];
};
