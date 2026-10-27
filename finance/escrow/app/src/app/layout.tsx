import type { Metadata } from "next";
import localFont from "next/font/local";
import type { ReactNode } from "react";
import { AppProviders } from "@/components/app-providers";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import "./globals.css";

// The book's text and code faces, served with the app. Its heading face,
// Denim INK, is licensed and named in globals.css instead.
const gelasio = localFont({
  src: [
    { path: "./fonts/Gelasio-Regular.ttf", weight: "400", style: "normal" },
    { path: "./fonts/Gelasio-Italic.ttf", weight: "400", style: "italic" },
    { path: "./fonts/Gelasio-Bold.ttf", weight: "700", style: "normal" },
    { path: "./fonts/Gelasio-BoldItalic.ttf", weight: "700", style: "italic" },
  ],
  variable: "--font-gelasio",
  display: "swap",
});

const dejavuMono = localFont({
  src: [
    { path: "./fonts/DejaVuSansMono.ttf", weight: "400", style: "normal" },
    { path: "./fonts/DejaVuSansMono-Bold.ttf", weight: "700", style: "normal" },
  ],
  variable: "--font-dejavu-mono",
  display: "swap",
});

export const metadata: Metadata = {
  title: {
    default: "Escrow",
    template: "%s · Escrow",
  },
  description:
    "Swap one token for another with a stranger, with the escrow program from Building Financial Software on Solana holding both sides.",
};

const RootLayout = ({ children }: { children: ReactNode }) => (
  <html lang="en" className={`${gelasio.variable} ${dejavuMono.variable}`}>
    <body>
      <AppProviders>
        <SiteHeader />
        <main className="page">{children}</main>
        <SiteFooter />
      </AppProviders>
    </body>
  </html>
);

export default RootLayout;
