// Token amounts live onchain as whole numbers of a token's smallest unit: 250
// USDC, at 6 decimals, is 250_000_000. These helpers convert between that and
// the decimal text people read and type, without ever going through a
// floating-point number, which cannot hold every u64.

const U64_MAX = 2n ** 64n - 1n;

export class AmountError extends Error {}

// "250.5" at 6 decimals becomes 250_500_000n.
export const parseAmount = (text: string, decimals: number): bigint => {
  const trimmed = text.trim().replaceAll(",", "");
  if (!/^\d*\.?\d*$/.test(trimmed) || trimmed === "" || trimmed === ".") {
    throw new AmountError("Enter a number, like 250 or 0.5");
  }
  const [whole = "", fraction = ""] = trimmed.split(".");
  if (fraction.length > decimals) {
    throw new AmountError(
      decimals === 0
        ? "This token has no decimal places"
        : `This token has ${decimals} decimal places, so use at most ${decimals} digits after the point`,
    );
  }
  const minorUnits = BigInt(`${whole || "0"}${fraction.padEnd(decimals, "0")}`);
  if (minorUnits > U64_MAX) {
    throw new AmountError("That is more than a token account can hold");
  }
  return minorUnits;
};

// 250_500_000n at 6 decimals becomes "250.5", with thousands grouped.
export const formatAmount = (minorUnits: bigint, decimals: number): string => {
  const scale = 10n ** BigInt(decimals);
  const whole = minorUnits / scale;
  const fraction = (minorUnits % scale).toString().padStart(decimals, "0").replace(/0+$/, "");
  const groupedWhole = whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return fraction === "" ? groupedWhole : `${groupedWhole}.${fraction}`;
};

// How many of `quoteAmount`'s token one whole `baseAmount` token costs. For an
// offer of 250 USDC for 1 TSLAx, the price of a TSLAx is 250 USDC, and the
// price of a USDC is 0.004 TSLAx. Prices are for reading and sorting only:
// the program never computes one, it swaps the two exact amounts.
export const unitPrice = (
  baseAmount: bigint,
  baseDecimals: number,
  quoteAmount: bigint,
  quoteDecimals: number,
): number => {
  if (baseAmount === 0n) {
    return Number.POSITIVE_INFINITY;
  }
  // Scale up before dividing so the integer division keeps 18 digits of
  // precision, then convert the much smaller result to a number.
  const PRECISION = 18n;
  const scaled =
    (quoteAmount * 10n ** BigInt(baseDecimals) * 10n ** PRECISION) / (baseAmount * 10n ** BigInt(quoteDecimals));
  return Number(scaled) / 10 ** Number(PRECISION);
};

export const formatPrice = (price: number): string => {
  if (!Number.isFinite(price)) {
    return "-";
  }
  return price.toLocaleString("en-US", { maximumSignificantDigits: 6 });
};
