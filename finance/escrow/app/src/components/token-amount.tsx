import { formatAmount } from "@/lib/amounts";
import type { MintInfo } from "@/lib/tokens";

type TokenAmountProps = {
  amount: bigint;
  mint: MintInfo;
  size?: "normal" | "large";
  // Marks an amount that a transaction moves, in the accent color, as the
  // book's figures do.
  moving?: boolean;
};

export const TokenAmount = ({ amount, mint, size = "normal", moving = false }: TokenAmountProps) => {
  const classNames = ["token-amount"];
  if (size === "large") {
    classNames.push("token-amount--large");
  }
  if (moving) {
    classNames.push("token-amount--moving");
  }
  return (
    <span className={classNames.join(" ")} title={`${mint.name} (${mint.address})`}>
      {formatAmount(amount, mint.decimals)} <span className="token-amount__symbol">{mint.symbol}</span>
    </span>
  );
};

// The book's mark for tokens moving: three overlapping rings.
export const CoinMark = () => (
  <svg className="coin-mark" viewBox="-11 -6 22 13" aria-hidden="true">
    <g fill="var(--color-paper)" stroke="currentColor" strokeWidth="1.3">
      <circle cx="5" cy="1.5" r="4.5" />
      <circle cx="0" cy="0" r="4.5" />
      <circle cx="-5" cy="1.5" r="4.5" />
    </g>
  </svg>
);
