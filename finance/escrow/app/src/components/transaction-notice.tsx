import type { ReactNode } from "react";
import { describeTransactionError, ensureError } from "@/lib/errors";
import { AddressLink } from "./address-link";

// Every log line the program printed lives on the failed simulation, a few
// causes down from the error a send throws.
const findProgramLogs = (thrownObject: unknown): Array<string> => {
  let current: unknown = ensureError(thrownObject);
  while (current instanceof Error) {
    const context = (current as Error & { context?: { logs?: Array<string> } }).context;
    if (context?.logs && context.logs.length > 0) {
      return context.logs;
    }
    current = current.cause;
  }
  return [];
};

export const TransactionError = ({ error, title }: { error: unknown; title: string }) => {
  const logs = findProgramLogs(error);
  return (
    <div className="notice notice--error" role="alert">
      <h3 className="notice__title">{title}</h3>
      <p>{describeTransactionError(error)}</p>
      {logs.length > 0 ? (
        <details className="program-logs">
          <summary>Program logs</summary>
          <pre className="program-logs__list">{logs.join("\n")}</pre>
        </details>
      ) : null}
    </div>
  );
};

type TransactionSuccessProps = {
  title: string;
  signature: string;
  children?: ReactNode;
  actions?: ReactNode;
};

export const TransactionSuccess = ({ title, signature, children, actions }: TransactionSuccessProps) => (
  <div className="notice notice--success" role="status">
    <h3 className="notice__title">{title}</h3>
    {children}
    <p>
      Transaction <AddressLink address={signature} kind="tx" />
    </p>
    {actions ? <div className="notice__actions">{actions}</div> : null}
  </div>
);
