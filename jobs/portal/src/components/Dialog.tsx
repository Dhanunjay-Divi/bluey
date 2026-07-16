import { useEffect, useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

const dialogStack: string[] = [];
const dialogElements = new Map<string, HTMLElement>();
let previousBodyOverflow = "";
let rootHadInert = false;
let previousRootAriaHidden: string | null = null;

function registerDialog(id: string, element: HTMLElement): void {
  if (dialogStack.length === 0) {
    previousBodyOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const root = document.getElementById("root");
    if (root) {
      rootHadInert = root.hasAttribute("inert");
      previousRootAriaHidden = root.getAttribute("aria-hidden");
      root.setAttribute("inert", "");
      root.setAttribute("aria-hidden", "true");
    }
  }
  dialogStack.push(id);
  dialogElements.set(id, element);
}

function unregisterDialog(id: string): string | undefined {
  const index = dialogStack.lastIndexOf(id);
  if (index >= 0) dialogStack.splice(index, 1);
  dialogElements.delete(id);
  const nextTop = dialogStack.at(-1);
  if (dialogStack.length === 0) {
    document.body.style.overflow = previousBodyOverflow;
    const root = document.getElementById("root");
    if (root) {
      if (!rootHadInert) root.removeAttribute("inert");
      if (previousRootAriaHidden === null) root.removeAttribute("aria-hidden");
      else root.setAttribute("aria-hidden", previousRootAriaHidden);
    }
  }
  return nextTop;
}

function focusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(
    container.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((element) => !element.hasAttribute("hidden") && element.getAttribute("aria-hidden") !== "true");
}

export function Dialog({ open, title, description, children, onClose, size = "medium" }: {
  open: boolean;
  title: string;
  description?: string;
  children: ReactNode;
  onClose(): void;
  size?: "small" | "medium" | "large";
}) {
  const instanceId = useId();
  const titleId = `${instanceId}-title`;
  const descriptionId = `${instanceId}-description`;
  const dialogRef = useRef<HTMLElement | null>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    const dialog = dialogRef.current;
    if (!dialog) return;
    returnFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    registerDialog(instanceId, dialog);
    const focusFrame = window.requestAnimationFrame(() => {
      if (dialogStack.at(-1) !== instanceId) return;
      const preferred = dialog.querySelector<HTMLElement>(
        '[data-dialog-initial-focus], input:not([disabled]), select:not([disabled]), textarea:not([disabled])',
      );
      (preferred ?? focusableElements(dialog)[0] ?? dialog).focus();
    });
    const handleKeyDown = (event: KeyboardEvent) => {
      if (dialogStack.at(-1) !== instanceId) return;
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = focusableElements(dialog);
      if (focusable.length === 0) {
        event.preventDefault();
        dialog.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      window.cancelAnimationFrame(focusFrame);
      document.removeEventListener("keydown", handleKeyDown);
      const nextTop = unregisterDialog(instanceId);
      if (nextTop) {
        window.requestAnimationFrame(() => dialogElements.get(nextTop)?.focus());
      } else {
        window.requestAnimationFrame(() => {
          if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus();
        });
      }
    };
  }, [instanceId, open]);

  if (!open) return null;
  return createPortal(
    <div
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && dialogStack.at(-1) === instanceId) {
          onCloseRef.current();
        }
      }}
    >
      <section
        ref={dialogRef}
        className={`dialog ${size}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descriptionId : undefined}
        tabIndex={-1}
      >
        <header>
          <div>
            <h2 id={titleId}>{title}</h2>
            {description && <p id={descriptionId}>{description}</p>}
          </div>
          <button type="button" className="icon-button" aria-label="Close" onClick={() => onCloseRef.current()}><X /></button>
        </header>
        {children}
      </section>
    </div>,
    document.body,
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
      <div className="dialog-actions"><button type="button" className="button secondary" onClick={onClose}>Cancel</button><button type="button" className={`button ${tone}`} onClick={onConfirm}>{confirmLabel}</button></div>
    </Dialog>
  );
}
