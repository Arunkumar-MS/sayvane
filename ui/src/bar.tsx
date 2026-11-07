import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/waveform.css";
import "./styles/bar.css";
import { BarApp } from "./bar/BarApp";
import { boot } from "./state/store";

void boot({ history: false });

createRoot(document.getElementById("bar-root")!).render(
  <StrictMode>
    <BarApp />
  </StrictMode>,
);
