import type { MintInfo } from "@/lib/tokens";
import { POPULAR_TOKENS } from "./popular-token-addresses";

// The logo bundled for each popular token, by mint address, on any cluster.
const LOGOS = new Map(
  Object.values(POPULAR_TOKENS)
    .flat()
    .flatMap((token) => (token.logo ? [[token.address, token.logo] as const] : [])),
);

// A token's logo, or for a token with no bundled logo, the first letter of
// its symbol in a circle. Decorative: the symbol beside it is the label.
export const TokenMark = ({ mint }: { mint: MintInfo }) => {
  const logo = LOGOS.get(mint.address);
  if (logo) {
    // biome-ignore lint/performance/noImgElement: a small static SVG needs no optimizing
    return <img className="token-mark" src={logo} alt="" />;
  }
  return (
    <span className="token-mark token-mark--letter" aria-hidden="true">
      {mint.isNamed ? mint.symbol.charAt(0) : "?"}
    </span>
  );
};
