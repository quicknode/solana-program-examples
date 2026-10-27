import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import type { NextConfig } from "next";

// The app is its own pnpm project inside the repository, which has a lockfile
// of its own at the root; this tells Next.js which one is the app's.
const appDirectory = dirname(fileURLToPath(import.meta.url));

const nextConfig: NextConfig = {
  reactStrictMode: true,
  turbopack: { root: appDirectory },
  outputFileTracingRoot: appDirectory,
  // The end-to-end tests build the app against a mock cluster into a
  // directory of their own, so they never overwrite a real build.
  distDir: process.env.ESCROW_APP_DIST_DIR || ".next",
};

export default nextConfig;
