import { defineConfig } from 'vite';

export default defineConfig({
  root: 'src',
  build: {
    sourcemap: true,
    target: 'esnext',
    outDir: '../dist',
  },
});
