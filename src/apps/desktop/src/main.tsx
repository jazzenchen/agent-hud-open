import React from "react";
import ReactDOM from "react-dom/client";
import "@agent-hud/app/styles.css";
import App from "@agent-hud/app";
import { StartupReady } from "./StartupReady";
import { loadCatalog } from "@agent-hud/ui/catalogs";
import { Button } from "@agent-hud/ui/components/button";
import { interfaceLanguage } from "@agent-hud/ui/format";

const root = ReactDOM.createRoot(document.getElementById("root")!);
const surface = new URLSearchParams(location.search).get("surface");
const nativeSurface = surface === "hud" || surface === "picture";
// No loading text: this boundary also catches a saved language's catalog
// loading after the first render, where an English line would flash over
// the whole window.
function render(children: React.ReactNode) {
  root.render(
    <React.StrictMode>
      <React.Suspense fallback={null}>
        <StartupReady>{children}</StartupReady>
      </React.Suspense>
    </React.StrictMode>,
  );
}

if (nativeSurface) {
  // Pictures choose a language from their job. Both surfaces keep their own
  // loading and error paths rather than preloading the system language.
  render(<App />);
} else {
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
