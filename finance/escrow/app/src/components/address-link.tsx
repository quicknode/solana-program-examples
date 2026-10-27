import { explorerUrl } from "@/lib/config";
import { shortAddress } from "@/lib/tokens";

type AddressLinkProps = {
  address: string;
  kind?: "address" | "tx";
  full?: boolean;
};

// An address or transaction signature, linked to Solana Explorer on this
// app's cluster.
export const AddressLink = ({ address, kind = "address", full = false }: AddressLinkProps) => (
  <a
    className="address address-link"
    href={explorerUrl(kind, address)}
    target="_blank"
    rel="noreferrer"
    title={`${address} on Solana Explorer`}
  >
    {full ? address : shortAddress(address)}
  </a>
);
