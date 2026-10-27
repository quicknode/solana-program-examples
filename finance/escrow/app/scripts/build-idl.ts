// Builds the escrow program's IDL from the Anchor v1 copy of the program and
// writes it to idl/escrow.json.
//
// `anchor idl build` does the same, but this only needs Cargo: Anchor's IDL
// comes from host-side test functions that the `idl-build` feature adds to the
// program crate, each printing one section of the IDL between marker lines.
// No SBF toolchain is involved. The steps after the build are the ones the
// Anchor CLI applies: join the sections, set the program address, and shorten
// module paths like `escrow::state::offer::Offer` to `Offer`.
//
// The Anchor v2 copy in ../anchor has the same handlers, arguments, accounts
// and account layout, so the one IDL describes both.
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const appDirectory = join(dirname(fileURLToPath(import.meta.url)), "..");
const programDirectory = join(appDirectory, "..", "anchor-v1", "programs", "escrow");
const PROGRAM_ADDRESS = "qbuMdeYxYJXBjU6C6qFKjZKjXmrU83eDQomHdrch826";

const output = execFileSync(
  "cargo",
  ["test", "--lib", "--features", "idl-build", "__anchor_private_print_idl", "--", "--show-output", "--quiet"],
  {
    cwd: programDirectory,
    encoding: "utf8",
    env: { ...process.env, ANCHOR_IDL_BUILD_PROGRAM_PATH: programDirectory },
    stdio: ["ignore", "pipe", "inherit"],
    maxBuffer: 64 * 1024 * 1024,
  },
);

const readSection = (name: string): unknown => {
  const match = output.match(new RegExp(`--- IDL begin ${name} ---\\n([\\s\\S]*?)\\n--- IDL end ${name} ---`));
  if (!match) {
    throw new Error(`The IDL build printed no "${name}" section`);
  }
  return JSON.parse(match[1]);
};

const idl = readSection("program") as Record<string, unknown>;
idl.address = PROGRAM_ADDRESS;
idl.errors = readSection("errors");

// `escrow::state::offer::Offer` becomes `Offer`, everywhere it appears.
const shortened = JSON.stringify(idl, null, 2).replace(/"(?:[a-z_][a-z0-9_]*::)+([A-Za-z0-9_]+)"/g, '"$1"');

writeFileSync(join(appDirectory, "idl", "escrow.json"), `${shortened}\n`);
console.log("Wrote idl/escrow.json");
