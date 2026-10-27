import type { ReactNode } from "react";

type GuidePanelProps = {
  title: string;
  steps: Array<ReactNode>;
  footer?: ReactNode;
};

// The numbered instructions that sit beside each page's main content.
export const GuidePanel = ({ title, steps, footer }: GuidePanelProps) => (
  <aside className="guide-panel" aria-label={title}>
    <h2 className="guide-panel__title">{title}</h2>
    <ol className="step-list">
      {steps.map((step, index) => (
        // The steps are fixed text, so their order is their identity.
        // biome-ignore lint/suspicious/noArrayIndexKey: the list never reorders
        <li key={index} className="step-list__item">
          <span>{step}</span>
        </li>
      ))}
    </ol>
    {footer ? <div className="guide-panel__footer">{footer}</div> : null}
  </aside>
);
