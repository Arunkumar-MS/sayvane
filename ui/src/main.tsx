import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/primitives.css";
import "./styles/waveform.css";
import "./styles/app.css";
import "./styles/home.css";
import "./styles/history.css";
import "./styles/settings.css";
import { App } from "./app/App";
import { boot } from "./state/store";

void boot();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
