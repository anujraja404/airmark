import { ShieldCheck, X } from "lucide-react";
import type { ToolProposal } from "../types";

type ToolConfirmSheetProps = {
  proposal: ToolProposal;
  onApprove: () => void;
  onDismiss: () => void;
};

export function ToolConfirmSheet({
  proposal,
  onApprove,
  onDismiss,
}: ToolConfirmSheetProps) {
  return (
    <section className="tool-sheet" aria-label="Confirm local workflow">
      <header className="tool-sheet-header">
        <span>
          <ShieldCheck size={16} />
          Confirm Tool Run
        </span>
        <button aria-label="Dismiss tool confirmation" onClick={onDismiss}>
          <X size={15} />
        </button>
      </header>
      <div className="tool-sheet-body">
        <h2>{proposal.label}</h2>
        <p>{proposal.rationale}</p>
        <code>{proposal.commandPreview.join(" ")}</code>
      </div>
      <footer className="tool-sheet-actions">
        <button type="button" className="secondary-action" onClick={onDismiss}>
          Not Now
        </button>
        <button type="button" className="primary-action" onClick={onApprove}>
          Run Locally
        </button>
      </footer>
    </section>
  );
}
