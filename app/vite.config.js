import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // Two pages: the app, and the radial menu's overlay window.
  build: { rollupOptions: { input: { main: "index.html", overlay: "overlay.html" } } },
});
