import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { addCatalog } from "@agent-hud/ui/format";
import {
  accountReadLabel,
  missingReadingNotice,
  serviceNotice,
  nextExpiryLabel,
  webSignInOffer,
} from "../src/panel/account-labels.ts";

// The Chinese below is zh-Hans.json's.
const locales = join(import.meta.dirname, "../../protocol/locales");
for (const language of ["en", "zh-Hans"] as const)
  addCatalog(
    language,
    JSON.parse(readFileSync(join(locales, `${language}.json`), "utf8")),
  );

const now = Date.parse("2026-10-21T12:00:00Z");
const account = {
  id: "codex:a",
  provider: "codex",
  label: "me@example.com",
  plan: "pro",
  stale: false,
};

test("an account reads as current until a read fails, then says when it was last read", () => {
  assert.equal(
    accountReadLabel(
      { ...account, checkedAt: "2026-10-21T11:58:00Z" },
      now,
      "en",
    ),
    "Current account",
  );
  assert.equal(
    accountReadLabel(
      { ...account, checkedAt: "2026-10-21T11:58:00Z" },
      now,
      "zh-Hans",
    ),
    "当前账户",
  );
  assert.equal(
    accountReadLabel(
      { ...account, stale: true, checkedAt: "2026-10-21T09:00:00Z" },
      now,
      "en",
    ),
    "Last read 3h ago",
  );
  assert.equal(
    accountReadLabel(
      { ...account, stale: true, checkedAt: "2026-10-21T11:30:00Z" },
      now,
      "zh-Hans",
    ),
    "上次读取 30m 前",
  );
});

test("the first expiry is named only when every reset has one", () => {
  const credits = [
    { id: "a", expiresAt: "2026-11-02T08:00:00Z" },
    { id: "b", expiresAt: "2026-11-09T08:00:00Z" },
  ];
  assert.deepEqual(nextExpiryLabel({ availableCount: 2, credits }, "en"), {
    full: "First expires Nov 2",
    short: "Nov 2",
  });
  assert.equal(
    nextExpiryLabel({ availableCount: 3, credits }, "en").full,
    "Partial expiry details",
  );
  assert.equal(
    nextExpiryLabel({ availableCount: 1 }, "zh-Hans").full,
    "有效期未提供",
  );
});

test("a block without readings says why in its own provider's words", () => {
  const signedOut =
    "Claude Code is signed out. Run claude to sign in again to update the quota";
  const providers = [
    { id: "claude", notice: signedOut },
    { id: "codex", notice: null },
    { id: "cursor", notice: "  " },
  ];
  assert.equal(missingReadingNotice(providers, "claude"), signedOut);
  assert.equal(missingReadingNotice(providers, "codex"), null);
  assert.equal(missingReadingNotice(providers, "cursor"), null);
  assert.equal(missingReadingNotice(providers, "Claude"), null);
  assert.equal(missingReadingNotice(providers, "kimi"), null);
});

test("a vendor status shows only while one of the agent's components is not operational", () => {
  const providers = [
    {
      id: "claude",
      accountStatus: {
        status: "ready" as const,
        checkedAt: null,
        notice: null,
        service: {
          status: "partial_outage" as const,
          components: ["Claude API (api.anthropic.com)"],
          incident: "Elevated API errors",
          page: "https://status.claude.com",
          checkedAt: "2026-11-03T10:00:00Z",
        },
      },
    },
    {
      id: "codex",
      accountStatus: {
        status: "ready" as const,
        checkedAt: null,
        notice: null,
        service: null,
      },
    },
  ];
  assert.equal(
    serviceNotice(providers, "claude", "zh-Hans"),
    "官方状态：部分故障 · Elevated API errors",
  );
  assert.equal(
    serviceNotice(providers, "claude", "en"),
    "Vendor status: partial outage · Elevated API errors",
  );
  assert.equal(serviceNotice(providers, "codex", "en"), null);
  assert.equal(serviceNotice(providers, "cursor", "en"), null);
});

test("a sign-in is offered until a site has a session, and again when it ends", () => {
  const sites = (signedIn: boolean[]) => [
    { host: "qoder.com", name: "Qoder", signedIn: signedIn[0] },
    { host: "qoder.com.cn", name: "Qoder CN", signedIn: signedIn[1] },
  ];
  const providers = (signedIn: boolean[], failure: string | null = null) => [
    {
      id: "qoder-cn",
      accountStatus: {
        status: "unavailable" as const,
        checkedAt: null,
        notice: null,
        failure: failure as "signedOut" | null,
        webSignIn: { provider: "qoder", sites: sites(signedIn) },
      },
    },
  ];
  // Nobody signed in: every site, offered for the provider that reads it.
  assert.deepEqual(webSignInOffer(providers([false, false]), "qoder-cn"), {
    provider: "qoder",
    sites: [
      { host: "qoder.com", name: "Qoder" },
      { host: "qoder.com.cn", name: "Qoder CN" },
    ],
  });
  // Signed in on one: nothing to offer.
  assert.equal(webSignInOffer(providers([true, false]), "qoder-cn"), null);
  // The kept session ended: that site again.
  assert.deepEqual(
    webSignInOffer(providers([true, false], "signedOut"), "qoder-cn"),
    { provider: "qoder", sites: [{ host: "qoder.com", name: "Qoder" }] },
  );
  assert.equal(webSignInOffer(providers([false, false]), "claude"), null);
});
