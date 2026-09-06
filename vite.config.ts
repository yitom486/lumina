import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
  // Dev cold start: pre-bundle heavy deps at server boot (parallel + cached
  // in node_modules/.vite) instead of discovering them one by one during the
  // first page load (that waterfall was the multi-second dev white screen).
  // Production builds are unaffected (rollup path).
  optimizeDeps: {
    include: [
      "react",
      "react-dom",
      "react-dom/client",
      "zustand",
      "zustand/middleware",
      "@tanstack/react-query",
      "react-markdown",
      "remark-gfm",
      "remark-math",
      "rehype-katex",
      "katex",
      "lucide-react",
      "clsx",
      "tailwind-merge",
      "class-variance-authority",
      "@radix-ui/react-alert-dialog",
      "@radix-ui/react-dialog",
      "@radix-ui/react-dropdown-menu",
      "@radix-ui/react-label",
      "@radix-ui/react-scroll-area",
      "@radix-ui/react-separator",
      "@radix-ui/react-slider",
      "@radix-ui/react-slot",
      "@radix-ui/react-tooltip",
      "@tauri-apps/api",
      "@tauri-apps/api/core",
      "@tauri-apps/plugin-dialog",
      "@tauri-apps/plugin-opener",
    ],
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: "unit",
          environment: "node",
          include: ["src/**/*.test.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "ui",
          environment: "happy-dom",
          include: ["src/**/*.test.tsx"],
          setupFiles: ["src/test/setup.ts"],
        },
      },
    ],
  },
});
