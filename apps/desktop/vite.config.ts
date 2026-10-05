/// <reference types="vitest/config" />
import { resolve } from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react-swc";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(({ mode }) => ({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "safari16",
    emptyOutDir: true,
    copyPublicDir: true,
    sourcemap: mode === "development",
    minify: mode === "development" ? false : "esbuild",
    rollupOptions: {
      output: {
        entryFileNames: "js/[hash].js",
        chunkFileNames: "js/[hash].js",
        assetFileNames: (assetInfo) => {
          const name = assetInfo.names?.[0];
          if (name) {
            const ext = name.split(".").pop() ?? "";
            if (/png|jpe?g|svg|webp|gif|tiff|bmp|ico/i.test(ext)) {
              return "images/[hash][extname]";
            }
            if (/css/i.test(ext)) {
              return "styles/[hash][extname]";
            }
            if (/woff2?|eot|ttf|otf/i.test(ext)) {
              return "fonts/[hash][extname]";
            }
          }
          return "assets/[hash][extname]";
        },
      },
    },
  },
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test-setup.ts"],
  },
}));
