import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

export default defineConfig({
  plugins: [viteSingleFile()],
  server: {
    port: 3000,
  },
  build: {
    assetsInlineLimit: Infinity,
  },
});
