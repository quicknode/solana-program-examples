// Codama client generator.
//
// Reads the Anchor IDL (src/idl/vault_strategy.json, itself produced by
// `anchor idl build`) and emits a Solana Kit TypeScript client into
// src/generated/. The app imports instruction builders and account decoders
// from there rather than hand-writing them.
//
// Regenerate after any change to the program:
//   pnpm generate-client
//
// The output is generated code, so it is not hand-edited and biome.jsonc
// already excludes `**/generated` from linting.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { type AnchorIdl, rootNodeFromAnchor } from "@codama/nodes-from-anchor";
import { renderVisitor } from "@codama/renderers-js";
import { createFromRoot } from "codama";

const here = dirname(fileURLToPath(import.meta.url));
const idlPath = join(here, "src", "idl", "vault_strategy.json");
const outDir = join(here, "src", "generated");

const idl = JSON.parse(readFileSync(idlPath, "utf-8")) as AnchorIdl;

const codama = createFromRoot(rootNodeFromAnchor(idl));

// renderVisitor's first argument is a *package* folder: by default it writes a
// package.json and nests the client under `<folder>/src/generated`. This app
// consumes the client by relative import rather than as a package, so flatten the
// output into src/generated itself and skip the package.json.
await codama.accept(
  renderVisitor(outDir, {
    deleteFolderBeforeRendering: true,
    generatedFolder: ".",
    syncPackageJson: false,
  }),
);

console.log(`Codama: generated TypeScript client in ${outDir}`);
