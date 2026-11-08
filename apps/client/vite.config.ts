import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri espera uma porta fixa; `strictPort` falha em vez de trocar de porta em silêncio.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 36480, strictPort: true },
  build: { target: "es2022" },
});
