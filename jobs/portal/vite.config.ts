import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { portalManualChunk } from "./bundle-policy";

export default defineConfig({
  base: "/jobs/",
  plugins: [react(), tailwindcss()],
  server: {
    host: "127.0.0.1",
    port: 5187,
    strictPort: true,
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/auth": "http://127.0.0.1:8080",
      "/account": "http://127.0.0.1:8080"
    }
  },
  build: {
    outDir: "../../web/jobs",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks: portalManualChunk,
        onlyExplicitManualChunks: true
      }
    }
  }
});
