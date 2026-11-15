import React from "react";
import ReactDOM from "react-dom/client";
import "@agent-hud/app/styles.css";
import App from "@agent-hud/app";
import { loadCatalog } from "@agent-hud/ui/catalogs";
import { Button } from "@agent-hud/ui/components/button";
import { interfaceLanguage } from "@agent-hud/ui/format";
import { StartupReady } from "./StartupReady";

const root = ReactDOM.createRoot(document.getElementById("root")!);
function render(children: React.ReactNode) {
  root.render(
    <React.StrictMode>
      <React.Suspense fallback={null}>
        <StartupReady>{children}</StartupReady>
      </React.Suspense>
    </React.StrictMode>,
  );
}

const surface = new URLSearchParams(location.search).get("surface");
if (surface === "hud" || surface === "picture") {
  // These surfaces own their loading and error paths. A picture takes its
  // language from the supplied job, independently of the system language.
  render(<App />);
} else {
  // Keep the document shell while the system language loads, then commit the
  // first app tree without entering Suspense for an already-ready catalog.
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
