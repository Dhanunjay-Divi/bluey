import { useEffect, type ReactNode } from "react";
import { X } from "lucide-react";

export function Dialog({ open, title, description, children, onClose, size = "medium" }: {
  open: boolean;
  title: string;
  description?: string;
  children: ReactNode;
  onClose(): void;
  size?: "small" | "medium" | "large";
}) {
  useEffect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const close = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    document.addEventListener("keydown", close);
    return () => {
      document.body.style.overflow = previous;
      document.removeEventListener("keydown", close);
    };
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className={`dialog ${size}`} role="dialog" aria-modal="true" aria-labelledby="dialog-title">
        <header>
          <div><h2 id="dialog-title">{title}</h2>{description && <p>{description}</p>}</div>
          <button className="icon-button" aria-label="Close" onClick={onClose}><X /></button>
        </header>
        {children}
      </section>
    </div>
  );
}

export function ConfirmDialog({ open, title, description, confirmLabel, tone = "primary", onConfirm, onClose }: {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  tone?: "primary" | "danger";
  onConfirm(): void;
  onClose(): void;
}) {
  return (
    <Dialog open={open} title={title} description={description} onClose={onClose} size="small">
      <div className="dialog-actions"><button className="button secondary" onClick={onClose}>Cancel</button><button className={`button ${tone}`} onClick={onConfirm}>{confirmLabel}</button></div>
    </Dialog>
  );
}
