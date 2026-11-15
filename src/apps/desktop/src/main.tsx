import React from "react";
import ReactDOM from "react-dom/client";
import "@agent-hud/app/styles.css";
import App from "@agent-hud/app";
import { StartupReady } from "./StartupReady";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <React.Suspense fallback={null}>
      <StartupReady>
        <App />
      </StartupReady>
    </React.Suspense>
  </React.StrictMode>,
);
