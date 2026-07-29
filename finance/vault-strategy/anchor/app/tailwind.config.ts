import type { Config } from "tailwindcss";

// Dark matte "terminal" world. Flat surfaces, hairline dividers, one signal accent
// (amber), semantic green/red reserved for P&L numbers only. No glow, glass, gradient.
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        graphite: "#0B0D10", // canvas
        panel: "#111419", // lifted surface
        panel2: "#161A20", // input / nested surface
        line: "rgba(233, 230, 225, 0.09)", // hairline
        line2: "rgba(233, 230, 225, 0.16)", // stronger hairline
        ink: "#E8E6E1", // primary text (warm off-white)
        muted: "#9C9A94", // secondary text — warm-tinted from ink, ~6:1 on graphite
        faint: "#84827C", // tertiary labels — still ~4.7:1
        accent: "#F0B429", // signal amber (actions, focus, active)
        "accent-dim": "#8A6716",
        gain: "#41B883", // P&L up
        loss: "#E5595E", // P&L down
      },
      fontFamily: {
        sans: ['"Archivo"', "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ['"IBM Plex Mono"', "ui-monospace", "SFMono-Regular", "monospace"],
      },
      letterSpacing: {
        tightest: "-0.03em",
      },
      fontSize: {
        // Hero readouts (AUM / NAV / position) live here.
        hero: ["3.25rem", { lineHeight: "1", letterSpacing: "-0.02em" }],
        stat: ["2rem", { lineHeight: "1.05", letterSpacing: "-0.01em" }],
      },
    },
  },
  plugins: [],
} satisfies Config;
