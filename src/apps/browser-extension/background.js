// The extension's worker: the toolbar button (or its shortcut) starts pick
// mode on the tab, and the page's picks go to Agent HUD through Chrome's
// native messaging. Agent HUD writes the host's note for this browser at
// each launch; the host is the app itself.
const HOST = "app.agenthud.inspect";

/** The browser's own name, for Agent HUD's Settings. */
const browserName = () =>
  (navigator.userAgentData?.brands ?? []).find(
    (b) => !/Not.?A.?Brand|Chromium/i.test(b.brand),
  )?.brand ?? "Chrome";

// Agent HUD shows in Settings that the extension can reach it.
const hello = () =>
  chrome.runtime
    .sendNativeMessage(HOST, { type: "hello", browser: browserName() })
    .catch(() => {});
chrome.runtime.onInstalled.addListener(hello);
chrome.runtime.onStartup.addListener(hello);

chrome.action.onClicked.addListener(async (tab) => {
  if (tab.id === undefined) return;
  try {
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      files: ["describe.js", "pick.js"],
    });
  } catch {
    // Chrome's own pages and the Web Store take no scripts.
    await chrome.action.setBadgeText({ tabId: tab.id, text: "×" });
    setTimeout(
      () => chrome.action.setBadgeText({ tabId: tab.id, text: "" }),
      2500,
    );
  }
});

chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (message?.type === "capture") {
    chrome.tabs.captureVisibleTab(sender.tab?.windowId, { format: "png" }).then(
      (url) => reply({ url }),
      (error) => reply({ error: String(error) }),
    );
    return true;
  }
  if (message?.type === "send") {
    chrome.runtime
      .sendNativeMessage(HOST, { type: "pick", pick: message.pick })
      .then(
        (answer) => reply(answer ?? { ok: false, error: "no answer" }),
        (error) =>
          reply({
            ok: false,
            unreachable: true,
            error: String(error?.message ?? error),
          }),
      );
    return true;
  }
  return false;
});
