
import { defineConfig, loadEnv } from "vite";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

// https://vitejs.dev/config/
export default defineConfig(({ mode }) => {
  const publicEnv = loadEnv(mode, repoRoot, ['NEXT_PUBLIC_', 'VITE_SUPABASE_']);
  const supabaseUrl = publicEnv.NEXT_PUBLIC_SUPABASE_URL || publicEnv.VITE_SUPABASE_URL || '';
  const supabaseAnonKey = publicEnv.NEXT_PUBLIC_SUPABASE_ANON_KEY || publicEnv.VITE_SUPABASE_PUBLIC_KEY || '';

  return {
    define: {
      'import.meta.env.NEXT_PUBLIC_SUPABASE_URL': JSON.stringify(supabaseUrl),
      'import.meta.env.NEXT_PUBLIC_SUPABASE_ANON_KEY': JSON.stringify(supabaseAnonKey),
    },
    root: 'src/renderer',
    publicDir: 'public',
    envDir: repoRoot,
    envPrefix: ['VITE_', 'NEXT_PUBLIC_'],
    server: {
      host: '127.0.0.1',
      port: 1420,
      strictPort: true,
    },
    build: {
      outDir: '../../dist',
      emptyOutDir: true,
    },
  };
});
