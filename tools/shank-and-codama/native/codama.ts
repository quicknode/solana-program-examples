// Codama client generator.
//
// Reads the Shank-generated IDL (program/idl/car_rental_service.json) and emits
// a TypeScript client built on @solana/kit into tests/generated/.
//
// Flow: read IDL -> rootNodeFromAnchor (origin = "shank" so the u8 instruction
// discriminants are interpreted correctly) -> createFromRoot -> render JS.
//
// Run with: pnpm generate-client

import { readFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { rootNodeFromAnchor, type AnchorIdl } from "@codama/nodes-from-anchor";
import { renderVisitor } from "@codama/renderers-js";
import { createFromRoot } from "codama";

const here = dirname(fileURLToPath(import.meta.url));
const idlPath = join(here, "program", "idl", "car_rental_service.json");
const outDir = join(here, "tests", "generated");

const idl = JSON.parse(readFileSync(idlPath, "utf-8")) as AnchorIdl;

// Make sure Codama treats this as a Shank IDL. Shank uses single-byte (u8)
// instruction discriminants rather than Anchor's 8-byte hashes, and the
// "origin" field is what tells nodes-from-anchor to honour the explicit
// `discriminant` values in the IDL.
const idlWithOrigin = {
  ...idl,
  metadata: { ...idl.metadata, origin: "shank" },
} as AnchorIdl;

const codama = createFromRoot(rootNodeFromAnchor(idlWithOrigin));

await codama.accept(renderVisitor(outDir, { deleteFolderBeforeRendering: true }));

// The renderer drops a standalone `package.json` (declaring an implicit CommonJS
// package) at the output root. That would shadow this example's
// `"type": "module"` setting and break ESM resolution of the generated `.ts`
// files when the test imports them via tsx, so remove it.
rmSync(join(outDir, "package.json"), { force: true });

console.log(`Codama: generated TypeScript client in ${outDir}`);
