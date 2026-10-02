import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Local assets only: no CDN, no remote fonts. The dev server is for the
// Tauri dev build; the shipped app embeds `dist/`.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "127.0.0.1" },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true, sourcemap: false },
});
