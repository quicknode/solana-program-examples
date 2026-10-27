"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { CLUSTER, IS_TEST_CLUSTER } from "@/lib/config";
import { WalletMenu } from "./wallet-menu";

const NAV_LINKS: Array<{ href: string; label: string; testClustersOnly?: boolean }> = [
  { href: "/", label: "Offers" },
  { href: "/make", label: "Make an offer" },
  { href: "/test-tokens", label: "Test tokens", testClustersOnly: true },
  { href: "/how-it-works", label: "How it works" },
];

const isCurrent = (pathname: string, href: string) =>
  href === "/" ? pathname === "/" || pathname.startsWith("/offers") : pathname.startsWith(href);

export const SiteHeader = () => {
  const pathname = usePathname();
  return (
    <header className="site-header">
      <div className="site-header__inner">
        <Link className="site-brand" href="/">
          <span className="site-brand__name">Escrow</span>
          <span className="site-brand__series">Building Financial Software on Solana</span>
        </Link>
        <nav className="site-nav" aria-label="Main">
          {NAV_LINKS.filter((link) => IS_TEST_CLUSTER || !link.testClustersOnly).map((link) => (
            <Link
              key={link.href}
              href={link.href}
              className={`site-nav__link${isCurrent(pathname, link.href) ? " site-nav__link--current" : ""}`}
              aria-current={isCurrent(pathname, link.href) ? "page" : undefined}
            >
              {link.label}
            </Link>
          ))}
        </nav>
        <div className="site-header__wallet">
          <span className="cluster-badge" title="The Solana cluster this app talks to">
            {CLUSTER}
          </span>
          <WalletMenu />
        </div>
      </div>
    </header>
  );
};
