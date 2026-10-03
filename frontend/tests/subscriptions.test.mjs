import assert from "node:assert/strict";
import test from "node:test";
import { subscriptionLabel, subscriptionChoices } from "../src/lib/subscriptions.ts";

test("subscription names match official numbered Pro tiers without exposing unknown identifiers", () => {
  assert.equal(subscriptionLabel("prolite"), "ChatGPT Pro 100");
  assert.equal(subscriptionLabel("pro"), "ChatGPT Pro 200");
  assert.equal(subscriptionLabel("promax"), "ChatGPT Pro 500");
  assert.equal(subscriptionLabel("team"), "ChatGPT Business");
  assert.equal(subscriptionLabel("internal_new_plan"), "未知订阅");
  assert.equal(subscriptionChoices.length, 9);
});
