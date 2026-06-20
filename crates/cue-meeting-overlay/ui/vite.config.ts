import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Lean config — no Tailwind (the Aurora Glass system is hand-authored CSS),
// no router (a single window switches views by state). Mirrors the dashboard's
// Tauri-friendly build (esnext, sourcemaps under TAURI_DEBUG).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5174, strictPort: true },
  build: {
    outDir: "dist",
    target: "esnext",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
