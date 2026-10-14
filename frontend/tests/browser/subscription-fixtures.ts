import { expect, type APIRequestContext } from "@playwright/test";

export async function createUser(
  request: APIRequestContext,
  headers: Record<string, string>,
  input: { username: string; name: string; email: string },
) {
  const response = await request.post("/admin/api/users", {
    headers,
    data: { ...input, password: "test-only-password", enabled: true, revision: null },
  });
  expect(response.status()).toBe(200);
  return (await response.json()) as typeof input & { id: string };
}

export async function grantSubscription(
  request: APIRequestContext,
  headers: Record<string, string>,
  userId: string,
  planId: string,
) {
  const response = await request.post("/admin/api/subscriptions", {
    headers,
    data: {
      user_id: userId,
      plan_id: planId,
      expires_at: null,
      enabled: true,
      reissue: false,
      revision: null,
    },
  });
  expect(response.status()).toBe(200);
  return (await response.json()) as { id: string };
}
