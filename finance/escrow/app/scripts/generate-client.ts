// Generates the escrow program's TypeScript client from idl/escrow.json into
// src/generated, using Codama. The generated client exports `escrowProgram()`,
// a Kit plugin that adds the program's instructions, accounts and PDAs to a
// Kit client as `client.escrow`.
//
// Anchor's IDL does not record the offer account's seeds, so they are added
// here: an offer lives at the PDA of ["offer", maker, id], with `id` as eight
// little-endian bytes, as `make_offer.rs` declares.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { type AnchorIdl, rootNodeFromAnchor } from "@codama/nodes-from-anchor";
import { renderVisitor } from "@codama/renderers-js";
import {
  accountValueNode,
  addPdasVisitor,
  argumentValueNode,
  constantPdaSeedNodeFromString,
  createFromRoot,
  numberTypeNode,
  pdaSeedValueNode,
  pdaValueNode,
  publicKeyTypeNode,
  publicKeyValueNode,
  setInstructionAccountDefaultValuesVisitor,
  variablePdaSeedNode,
} from "codama";

const appDirectory = join(dirname(fileURLToPath(import.meta.url)), "..");
const idl = JSON.parse(readFileSync(join(appDirectory, "idl", "escrow.json"), "utf8")) as AnchorIdl;

const codama = createFromRoot(rootNodeFromAnchor(idl));

codama.update(
  addPdasVisitor({
    escrow: [
      {
        name: "offer",
        seeds: [
          constantPdaSeedNodeFromString("utf8", "offer"),
          variablePdaSeedNode("maker", publicKeyTypeNode()),
          variablePdaSeedNode("id", numberTypeNode("u64")),
        ],
      },
    ],
  }),
);

// The associated token program has one address on every cluster, so no
// caller should have to pass it.
//
// make_offer knows the offer's id, so the client can derive the offer address
// itself. take_offer and cancel_offer act on an offer the caller already found,
// so they take its address.
codama.update(
  setInstructionAccountDefaultValuesVisitor([
    {
      account: "associatedTokenProgram",
      defaultValue: publicKeyValueNode("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL", "associatedTokenProgram"),
    },
    {
      account: "offer",
      instruction: "makeOffer",
      defaultValue: pdaValueNode("offer", [
        pdaSeedValueNode("maker", accountValueNode("maker")),
        pdaSeedValueNode("id", argumentValueNode("id")),
      ]),
    },
  ]),
);

await codama.accept(
  renderVisitor(appDirectory, {
    generatedFolder: "src/generated",
    kitImportStrategy: "rootOnly",
    syncPackageJson: false,
  }),
);
console.log("Wrote src/generated");
