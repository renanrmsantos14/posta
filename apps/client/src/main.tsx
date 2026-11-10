import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource-variable/newsreader";
import "./themes.css";
import "./styles.css";
import "./safeArea";
import App from "./App";

declare global {
  interface Window {
    /** Etapas da abertura para o logcat (public/boot-guard.js); ausente se o vigia não carregou. */
    postaBootLog?: (msg: string) => void;
  }
}

window.postaBootLog?.("ui: bundle iniciou");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
