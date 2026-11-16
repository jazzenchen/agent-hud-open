import React from "react";
import ReactDOM from "react-dom/client";
import "@agent-hud/app/styles.css";
import App from "@agent-hud/app";
import { loadCatalog } from "@agent-hud/ui/catalogs";
import { Button } from "@agent-hud/ui/components/button";
import { interfaceLanguage } from "@agent-hud/ui/format";

const root = ReactDOM.createRoot(document.getElementById("root")!);
const surface = new URLSearchParams(location.search).get("surface");
const nativeSurface = surface === "hud" || surface === "picture";
const loading = nativeSurface ? null : (
  <main role="status" style={{ padding: 32 }}>
    Loading Agent HUD…
  </main>
);
function render(children: React.ReactNode) {
  root.render(
    <React.StrictMode>
      <React.Suspense fallback={loading}>{children}</React.Suspense>
    </React.StrictMode>,
  );
}

if (nativeSurface) {
  // Pictures choose a language from their job. Both surfaces keep their own
  // loading and error paths rather than preloading the system language.
  render(<App />);
} else {
  render(loading);
  void Promise.resolve()
    .then(() => loadCatalog(interfaceLanguage("system")))
    .then(
      () => render(<App />),
      (error: unknown) => {
        console.error("Agent HUD startup catalog:", error);
        render(
          <main role="alert" style={{ padding: 32 }}>
            <h1>Couldn’t load Agent HUD</h1>
            <p>
              The interface language could not be loaded. Reload to try again.
            </p>
            <Button onClick={() => location.reload()}>Reload</Button>
          </main>,
        );
      },
    );
}
