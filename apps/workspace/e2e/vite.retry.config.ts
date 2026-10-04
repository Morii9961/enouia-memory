import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// A separate test artifact, outside production dist/. No alias, replacement
// IPC, production fault command, or extra Rust permission is introduced.
export default defineConfig({
  plugins: [react()],
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  build: {
    target: "es2022",
    outDir: "../../.local/mv6-retry-fixture",
    emptyOutDir: true,
    lib: { entry: "e2e/retry-fixture.tsx", name: "MV6RetryTest", formats: ["iife"], fileName: () => "retry-fixture.js" },
  },
});
