import { defineConfig } from "vite";

// L'interface vit dans ui/ ; Tauri attend le résultat dans dist/.
export default defineConfig({
  root: "ui",
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "../dist", emptyOutDir: true, target: "es2021" },
});
