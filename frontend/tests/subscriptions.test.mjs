import assert from "node:assert/strict";
import test from "node:test";
import {
  subscriptionLabel,
  subscriptionChoices,
} from "../src/lib/providers/chatgpt/subscriptions.ts";

test("subscription names match official numbered Pro tiers without exposing unknown identifiers", () => {
  assert.equal(subscriptionLabel("prolite"), "ChatGPT Pro 100");
  assert.equal(subscriptionLabel("pro"), "ChatGPT Pro 200");
  assert.equal(subscriptionLabel("promax"), "ChatGPT Pro 500");
  assert.equal(subscriptionLabel("team"), "ChatGPT Business");
  assert.equal(subscriptionLabel("internal_new_plan"), "未知订阅");
  assert.equal(subscriptionChoices.length, 9);
});

import { subscriptionLabel as grokLabel } from "../src/lib/providers/grok/subscriptions.ts";
import { supplierSubscriptionLabel } from "../src/lib/providers/grok/subscriptions.ts";
test("Grok preserves the observed subscription label including Free", () => {
  assert.equal(grokLabel("Free"), "Free");
  assert.equal(grokLabel("SuperGrokHeavy"), "SuperGrok Heavy");
  assert.equal(grokLabel("SuperGrokPro"), "SuperGrok Heavy");
  assert.equal(grokLabel("GrokPro"), "SuperGrok");
  assert.equal(grokLabel("SuperGrokLite"), "SuperGrok Lite");
  assert.equal(grokLabel("SuperGrokPlus"), "SuperGrok Plus");
  assert.equal(grokLabel("XBasic"), "X Basic");
  assert.equal(grokLabel("XPremium"), "X Premium");
  assert.equal(grokLabel("XPremiumPlus"), "X Premium+");
  assert.equal(grokLabel("Future official tier"), "Future official tier");
  assert.equal(supplierSubscriptionLabel("Free"), "Free");
  assert.equal(supplierSubscriptionLabel("SuperGrokPro"), "SuperGrokPro");
  assert.equal(supplierSubscriptionLabel("Official Future Label"), "Official Future Label");
});
