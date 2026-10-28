import { type ChildProcess, spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const appDirectory = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const nextBinary = join(appDirectory, "node_modules", ".bin", "next");
export const E2E_DIST_DIR = ".next-e2e";

const run = (args: Array<string>, env: NodeJS.ProcessEnv) =>
  new Promise<void>((resolve, reject) => {
    const child = spawn(nextBinary, args, { cwd: appDirectory, env, stdio: ["ignore", "ignore", "inherit"] });
    child.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`next ${args[0]} exited with ${code}`))));
  });

// Builds the app pointed at a mock cluster, then serves it. NEXT_PUBLIC_
// settings are fixed at build time, so the build has to know the cluster's
// address.
export const buildAndStartApp = async (rpcUrl: string, port: number): Promise<{ url: string; stop: () => void }> => {
  const env = {
    ...process.env,
    NEXT_PUBLIC_CLUSTER: "devnet",
    NEXT_PUBLIC_RPC_URL: rpcUrl,
    // The mock cluster stands in for mainnet's SNS accounts too.
    NEXT_PUBLIC_SNS_RPC_URL: rpcUrl,
    ESCROW_APP_DIST_DIR: E2E_DIST_DIR,
    NEXT_TELEMETRY_DISABLED: "1",
  };
  await run(["build"], env);
  const server: ChildProcess = spawn(nextBinary, ["start", "--port", String(port)], {
    cwd: appDirectory,
    env,
    stdio: ["ignore", "pipe", "inherit"],
  });
  const url = `http://127.0.0.1:${port}`;
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return { url, stop: () => server.kill() };
      }
    } catch {
      // Not listening yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  server.kill();
  throw new Error("The app did not start");
};
