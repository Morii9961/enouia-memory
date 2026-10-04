import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  build: {
    target: "es2022",
    outDir: "../../.local/mv6-read-fixture",
    emptyOutDir: true,
    lib: { entry: "e2e/read-fixture.tsx", name: "MV6ReadTest", formats: ["iife"], fileName: () => "read-fixture.js" },
  },
});
