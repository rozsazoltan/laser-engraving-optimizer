import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  build: {
    outDir: "dist",
    target: "es2022",
  },
  server: {
    port: 5173,
    strictPort: true,
  },
});
