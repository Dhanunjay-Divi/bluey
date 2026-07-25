import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

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
    // The lazy DOCX parser is intentionally isolated and loaded only during resume import.
    chunkSizeWarningLimit: 550,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (
            id.includes("/node_modules/react/") ||
            id.includes("/node_modules/react-dom/") ||
            id.includes("/node_modules/react-router/") ||
            id.includes("/node_modules/react-router-dom/") ||
            id.includes("/node_modules/scheduler/")
          ) {
            return "react-vendor";
          }
        }
      }
    }
  }
});
