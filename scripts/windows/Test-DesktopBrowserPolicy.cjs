"use strict";
// Consume real server test responses with the SDK and selectors in installed
// Desktop. This runs outside Desktop; it does not patch the installed client.
const fs = require("node:fs");
const vm = require("node:vm");
const assert = require("node:assert/strict");
const { archive, statsigSdk } = require("./desktop-contract.cjs");

const source = archive(process.argv[2]);
const shared = source.read(source.unique("webview/assets", "app-shared-"));
const sdk = statsigSdk(source);
const main = source.read(source.unique(".vite/build", "main-"));
const selectorSource = shared.match(
  /function (\w+)\(\{areRequirementsPending:[^}]+isBrowserAgentGateEnabled:[^}]+\}\)\{[^}]+\}/,
);
assert(
  selectorSource,
  "Installed built-in browser availability reader changed",
);
const select = vm.runInNewContext(selectorSource[0] + ";" + selectorSource[1]);
assert(
  shared.includes("name:`browser.in-app`"),
  "Installed browser capability changed",
);
assert.match(shared, /name:`browser\.in-app`[\s\S]{0,180}`410262010`/);
const selectionSource = main.match(
  /async function (\w+)\(\{appServerConnection:\w+,desktopFeatureAvailability:\w+,marketplaces:\w+,shouldUseWslPaths:\w+,\.\.\.\w+\}\)\{[^\n]{1,5000}?cuaReplSurfaces:\w+\}\}/,
);
assert(selectionSource, "Installed browser runtime selection changed");
const selectBackends = vm.runInNewContext(
  selectionSource[0] + ";" + selectionSource[1],
  {
    o: { at: () => "fixture-home" },
    r: { Zo: () => "fixture-marketplace" },
    u: { t: { resolve: () => "prod" } },
    Wi: () => ({ platform: "win32" }),
    Fne: () => true,
  },
);
const inputs = {
  areRequirementsPending: false,
  isBrowserAndComputerUseAllowed: true,
  isBrowserEnabled: true,
  isBrowserUseEnabled: true,
  isLoading: false,
  runCodexInWsl: false,
  windowType: "electron",
};
const hash = (name) =>
  String(
    [...name].reduce(
      (value, c) => (Math.imul(value, 31) + c.charCodeAt(0)) >>> 0,
      0,
    ),
  );
let sequence = 0;
async function evaluate(payload, expected) {
  const client = new sdk.StatsigClient(
    "browser-policy-test-" + sequence++,
    payload.user,
    {
      disableStorage: true,
      loggingEnabled: "disabled",
      networkConfig: { preventAllNetworkTraffic: true },
    },
  );
  try {
    client.dataAdapter.setData(JSON.stringify(payload));
    assert(
      client.initializeSync({ disableBackgroundCacheRefresh: true }).success,
    );
    const gate = client.checkGate("410262010");
    assert.equal(gate, expected);
    const availability = select({ ...inputs, isBrowserAgentGateEnabled: gate });
    assert.equal(availability, expected ? "available" : "statsig-disabled");
    const external = client.checkGate("410065390");
    assert.equal(external, false);
    const runtime = await selectBackends({
      appServerConnection: { appServerVersion: "fixture" },
      desktopFeatureAvailability: {
        externalBrowserUse: external,
        inAppBrowserUse: availability === "available",
        mcpAppsBrowserUse: false,
        computerUse: false,
      },
    });
    assert.deepEqual(
      Array.from(runtime.browserBackends),
      expected ? ["iab"] : [],
    );
  } finally {
    client.shutdown();
  }
}

async function mainTest() {
  const samples = JSON.parse(fs.readFileSync(process.argv[3] ?? 0, "utf8"));
  assert.deepEqual(
    samples.map((sample) => sample.enabled),
    [true, false, true],
  );
  for (const sample of samples) {
    await evaluate(sample.bootstrap, sample.enabled);
    await evaluate(sample.refresh, sample.enabled);
  }
  const old = structuredClone(samples[0].bootstrap);
  delete old.feature_gates[hash("410262010")];
  old.live_entity_names.feature_gates =
    old.live_entity_names.feature_gates.filter(
      (name) => name !== hash("410262010"),
    );
  await evaluate(old, false);

  for (const [overrides, reason] of [
    [{ areRequirementsPending: true }, "loading"],
    [{ isLoading: true }, "loading"],
    [{ isBrowserAndComputerUseAllowed: false }, "config-requirement-disabled"],
    [{ isBrowserUseEnabled: false }, "config-requirement-disabled"],
    [{ isBrowserEnabled: false }, "browser-pane-disabled"],
    [{ runCodexInWsl: true }, "wsl-disabled"],
    [{ windowType: "chrome-extension" }, "window-type-disabled"],
  ]) {
    assert.equal(
      select({ ...inputs, isBrowserAgentGateEnabled: true, ...overrides }),
      reason,
    );
  }
  console.log(
    "PASS: installed Desktop reproduces the missing-gate restriction; bootstrap/refresh select only iab, exclude Chrome, and preserve native restrictions. This is contract evidence, not a live browser task.",
  );
}
mainTest().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
