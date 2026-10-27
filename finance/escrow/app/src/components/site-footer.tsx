import { CLUSTER, ESCROW_PROGRAM, explorerUrl } from "@/lib/config";

export const SiteFooter = () => (
  <footer className="site-footer">
    <div className="site-footer__inner">
      <p className="site-footer__book">
        A companion to <a href="https://solanabook.org">Building Financial Software on Solana</a>, whose Escrow chapter
        explains this program line by line.
      </p>
      <p>
        Escrow program on {CLUSTER}:{" "}
        <a className="address address-link" href={explorerUrl("address", ESCROW_PROGRAM)}>
          {ESCROW_PROGRAM}
        </a>
        {" · "}
        <a href="https://github.com/quicknode/solana-program-examples/tree/main/finance/escrow">Source</a>
      </p>
    </div>
  </footer>
);
