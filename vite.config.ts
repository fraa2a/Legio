import tailwindcss from "@tailwindcss/vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";


export default defineConfig({
  plugins: [{
    name: "load-svelte-before-style",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use((request, _response, next) => {
        const url = (request as { url?: string }).url ?? "";
        if (!/\.svelte\?svelte&type=style&lang\.css$/.test(url)) return next();
        // The browser can request a cached style after the dev server restarts.
        void server.transformRequest(url.split("?")[0]).then(() => next(), next);
      });
    },
  }, tailwindcss(), svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
