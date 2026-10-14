import { defineConfig } from "vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import { nitro } from "nitro/vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const productionHost = process.env.VERCEL_PROJECT_PRODUCTION_URL;
const siteUrl = productionHost
  ? `https://${productionHost}`
  : "http://localhost:3000";

export default defineConfig({
  resolve: { tsconfigPaths: true },
  define: {
    "import.meta.env.SITE_URL": JSON.stringify(siteUrl),
  },
  plugins: [
    tanstackStart({ prerender: { enabled: true, failOnError: true } }),
    nitro({ preset: "vercel" }),
    react(),
    tailwindcss(),
  ],
});
