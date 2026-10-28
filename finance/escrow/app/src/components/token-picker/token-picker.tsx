"use client";

import { useId } from "react";
import type { MintInfo } from "@/lib/tokens";
import { TokenMark } from "./token-mark";

export const OTHER_TOKEN = "other";

export type TokenChoice = {
  mint: MintInfo;
  // Why this token cannot be chosen right now, or undefined if it can.
  disabledReason?: string;
};

type TokenPickerProps = {
  legend: string;
  choices: Array<TokenChoice>;
  // A mint address from `choices`, OTHER_TOKEN, or "" for nothing chosen.
  selected: string;
  onSelect: (choice: string) => void;
  otherMintText: string;
  onOtherMintTextChange: (text: string) => void;
  // What the address typed under "Other" turned out to be: a hint or an error.
  otherMintStatus: { kind: "hint" | "error"; text: string } | null;
};

// A row of tokens to pick from, one click each, ending in "Other", which
// opens a field for any mint address. Each token is a radio button styled as
// a chip, so the row is one choice that the keyboard's arrow keys move
// through.
export const TokenPicker = ({
  legend,
  choices,
  selected,
  onSelect,
  otherMintText,
  onOtherMintTextChange,
  otherMintStatus,
}: TokenPickerProps) => {
  const name = useId();
  const otherInputId = useId();
  // Tokens that can't be chosen, one line per reason.
  const disabledGroups = [
    ...choices
      .reduce((groups, { mint, disabledReason }) => {
        if (disabledReason) {
          groups.set(disabledReason, [...(groups.get(disabledReason) ?? []), mint.symbol]);
        }
        return groups;
      }, new Map<string, Array<string>>())
      .entries(),
  ];
  return (
    <fieldset className="token-picker">
      <legend className="field__label">{legend}</legend>
      <div className="token-choices">
        {choices.map(({ mint, disabledReason }) => (
          <label
            key={mint.address}
            className={`token-choice${disabledReason ? " token-choice--disabled" : ""}${
              selected === mint.address ? " token-choice--selected" : ""
            }`}
            title={disabledReason ?? `${mint.name} (${mint.address})`}
          >
            <input
              className="visually-hidden"
              type="radio"
              name={name}
              value={mint.address}
              checked={selected === mint.address}
              disabled={disabledReason !== undefined}
              onChange={() => onSelect(mint.address)}
            />
            <TokenMark mint={mint} />
            <span className="token-choice__symbol">{mint.symbol}</span>
          </label>
        ))}
        <label className={`token-choice${selected === OTHER_TOKEN ? " token-choice--selected" : ""}`}>
          <input
            className="visually-hidden"
            type="radio"
            name={name}
            value={OTHER_TOKEN}
            checked={selected === OTHER_TOKEN}
            onChange={() => onSelect(OTHER_TOKEN)}
          />
          <span className="token-choice__symbol">Other…</span>
        </label>
      </div>
      {disabledGroups.length > 0 ? (
        <ul className="token-picker__notes">
          {disabledGroups.map(([reason, symbols]) => (
            <li key={reason} className="field__hint">
              {symbols.join(", ")}: {reason}
            </li>
          ))}
        </ul>
      ) : null}
      {selected === OTHER_TOKEN ? (
        <div className="field">
          <label className="field__label" htmlFor={otherInputId}>
            Mint address
          </label>
          <input
            id={otherInputId}
            className="field__control address"
            autoComplete="off"
            spellCheck={false}
            placeholder="The token's mint address"
            value={otherMintText}
            onChange={(event) => onOtherMintTextChange(event.target.value)}
          />
          {otherMintStatus ? (
            <span className={otherMintStatus.kind === "error" ? "field__error" : "field__hint"}>
              {otherMintStatus.text}
            </span>
          ) : null}
        </div>
      ) : null}
    </fieldset>
  );
};
