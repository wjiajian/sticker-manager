import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // 让 Rust 的编译输出留在终端里
  clearScreen: false,
  build: {
    rollupOptions: {
      input: { main: "index.html", panel: "panel.html", ball: "ball.html", preview: "preview.html" },
    },
  },
  server: {
    // 与 ../src-tauri/tauri.conf.json 的 build.devUrl 保持一致
    port: 1420,
    strictPort: true,
    // 应用图标放在 src-tauri/icons，在 Vite 根目录之外，开发时要单独放行
    fs: { allow: [".", "../src-tauri/icons"] },
  },
});
