import { expect, test, type Page, type Route } from "@playwright/test";

/** Hold an actual shipped module so first paint and Suspense can be checked
 * without changing the app's code or replacing any React component. */
async function holdModule(page: Page, url: RegExp) {
  let signalRequest!: () => void;
  let releaseRequest!: () => void;
  const requested = new Promise<void>((resolve) => (signalRequest = resolve));
  const released = new Promise<void>((resolve) => (releaseRequest = resolve));
  const handler = async (route: Route) => {
    signalRequest();
    await released;
    await route.continue();
  };
  await page.route(url, handler);
  return { requested, release: releaseRequest };
}

test.beforeEach(async ({ page }) => {
  // The browser tests exercise startup without a runtime or real account data.
  await page.route("**/__bridge", (route) =>
    route.fulfill({
      status: 503,
      contentType: "application/json",
      body: JSON.stringify({ error: "Runtime unavailable in browser test." }),
    }),
  );
});

for (const theme of ["light", "dark"] as const) {
  test(`paints a quiet ${theme} shell before the application module loads`, async ({
    page,
  }) => {
    await page.emulateMedia({ colorScheme: theme });
    const entry = await holdModule(page, /\/assets\/index-[^/]+\.js$/);
    try {
      await page.goto("/", { waitUntil: "commit" });
      await entry.requested;
      const shell = page.locator("#startup-shell");
      await expect(shell).toBeVisible();
      await expect(page.locator("#root")).toBeEmpty();
      await expect(page.locator("body")).toHaveCSS(
        "background-color",
        theme === "light" ? "rgb(245, 245, 247)" : "rgb(40, 40, 40)",
      );
      await expect(shell).toHaveCSS(
        "background-color",
        theme === "light" ? "rgb(245, 245, 247)" : "rgb(40, 40, 40)",
      );
      const bounds = await shell.boundingBox();
      expect(bounds?.width).toBeGreaterThanOrEqual(1000);
      expect(bounds?.height).toBeGreaterThanOrEqual(700);
      expect(
        await shell.evaluate(
          (element) => element.getAnimations({ subtree: true }).length,
        ),
      ).toBe(0);
    } finally {
      entry.release();
    }
  });
}

for (const appearance of ["light", "dark"] as const) {
  test(`uses saved ${appearance} appearance before JavaScript, despite the opposite system theme`, async ({
    page,
  }) => {
    await page.emulateMedia({
      colorScheme: appearance === "light" ? "dark" : "light",
    });
    // Tauri sets these attributes at document creation. Keep the saved
    // preference independent of the browser's emulated system appearance.
    await page.addInitScript((theme) => {
      const apply = () => {
        if (!document.documentElement) return false;
        document.documentElement.dataset.initialAppearance = theme;
        document.documentElement.dataset.theme = theme;
        return true;
      };
      if (!apply()) {
        const observer = new MutationObserver(() => {
          if (apply()) observer.disconnect();
        });
        observer.observe(document, { childList: true, subtree: true });
      }
    }, appearance);
    const entry = await holdModule(page, /\/assets\/index-[^/]+\.js$/);
    try {
      await page.goto("/", { waitUntil: "commit" });
      await entry.requested;
      await expect(page.locator("#startup-shell")).toBeVisible();
      await expect(page.locator("#root")).toBeEmpty();
      await expect(page.locator("body")).toHaveCSS(
        "background-color",
        appearance === "light" ? "rgb(245, 245, 247)" : "rgb(40, 40, 40)",
      );
      await expect(page.locator("#startup-shell")).toHaveCSS(
        "background-color",
        appearance === "light" ? "rgb(245, 245, 247)" : "rgb(40, 40, 40)",
      );
    } finally {
      entry.release();
    }
  });
}

test("paints a shell for the Statistics window before JavaScript loads", async ({
  page,
}) => {
  const entry = await holdModule(page, /\/assets\/index-[^/]+\.js$/);
  try {
    await page.goto("/?window=statistics", { waitUntil: "commit" });
    await entry.requested;
    await expect(page.locator("#startup-shell")).toBeVisible();
    await expect(page.locator("#root")).toBeEmpty();
  } finally {
    entry.release();
  }
});

for (const query of [
  "surface=hud",
  "surface=picture",
  "surface=hud&window=statistics",
]) {
  test(`does not paint a window shell over ?${query}`, async ({ page }) => {
    const entry = await holdModule(page, /\/assets\/index-[^/]+\.js$/);
    try {
      await page.goto(`/?${query}`, { waitUntil: "commit" });
      await entry.requested;
      const shell = page.locator("#startup-shell");
      await expect(shell).toBeAttached();
      await expect(shell).toBeHidden();
      await expect(page.locator("#root")).toBeEmpty();
      await expect(page.locator("html")).toHaveCSS(
        "background-color",
        "rgba(0, 0, 0, 0)",
      );
      await expect(page.locator("body")).toHaveCSS(
        "background-color",
        "rgba(0, 0, 0, 0)",
      );
    } finally {
      entry.release();
    }
  });
}

test.describe("while the initial translation catalog is loading", () => {
  test.use({ locale: "zh-CN" });

  test("keeps the shell until translated application content commits", async ({
    page,
  }) => {
    const catalog = await holdModule(page, /\/assets\/zh-Hans-[^/]+\.js$/);
    try {
      await page.goto("/", { waitUntil: "domcontentloaded" });
      await catalog.requested;
      await expect(page.locator("#root")).toBeEmpty();
      await expect(page.locator("#startup-shell")).toBeVisible();
    } finally {
      catalog.release();
    }
    await expect(page.getByRole("navigation", { name: "设置" })).toBeVisible();
    await expect(page.locator("#startup-shell")).toBeHidden();
    await expect(page.locator("#startup-shell")).toHaveJSProperty(
      "hidden",
      true,
    );
  });

  test("reports a failed startup catalog and can reload after it becomes available", async ({
    page,
  }) => {
    const catalog = /\/assets\/zh-Hans-[^/]+\.js$/;
    await page.route(catalog, (route) => route.abort("failed"));
    await page.goto("/");
    await expect(page.getByRole("alert")).toContainText(
      "Couldn’t load Agent HUD",
    );
    await expect(page.locator("#startup-shell")).toBeHidden();
    await page.unroute(catalog);
    await page.getByRole("button", { name: "Reload" }).click();
    await expect(page.getByRole("navigation", { name: "设置" })).toBeVisible();
    await expect(page.locator("#startup-shell")).toBeHidden();
  });

  for (const surface of ["hud", "picture"]) {
    test(`does not introduce a startup error page on the ${surface} surface`, async ({
      page,
    }) => {
      let catalogRequests = 0;
      await page.route(/\/assets\/zh-Hans-[^/]+\.js$/, (route) => {
        catalogRequests += 1;
        return route.abort("failed");
      });
      if (surface === "picture") {
        // Hold the native job at its boundary; no system-language catalog
        // is needed while the real Picture component waits for its payload.
        await page.addInitScript(() => {
          Reflect.set(window, "__TAURI_INTERNALS__", {
            invoke(command: string) {
              if (command === "picture_payload") return new Promise(() => {});
              throw new Error(`Unexpected picture command: ${command}`);
            },
          });
        });
      }
      await page.goto(`/?surface=${surface}`);
      if (surface === "picture") {
        await expect(page.locator("#startup-shell")).toHaveJSProperty(
          "hidden",
          true,
        );
        expect(catalogRequests).toBe(0);
      } else {
        // The HUD's own FormatProvider may request the system language;
        // its failure must not add a normal window's error page to the HUD.
        await expect.poll(() => catalogRequests).toBeGreaterThan(0);
      }
      await expect(page.getByRole("alert")).toHaveCount(0);
      await expect(page.locator("#startup-shell")).toBeHidden();
    });
  }
});

test("replaces the startup shell with the English application", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.getByRole("navigation", { name: "Settings" }),
  ).toBeVisible();
  await expect(page.locator("#startup-shell")).toBeHidden();
  await expect(page.locator("#startup-shell")).toHaveJSProperty("hidden", true);
});
