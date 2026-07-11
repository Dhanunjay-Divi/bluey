import type { Locator, Page } from "playwright";
import type { BrowserLocator, BrowserPage, FormControl, FormControlKind } from "./contracts.js";

export class PlaywrightBrowserPage implements BrowserPage {
  constructor(readonly page: Page) {}

  url(): string {
    return this.page.url();
  }

  title(): Promise<string> {
    return this.page.title();
  }

  locator(selector: string): BrowserLocator {
    return new PlaywrightBrowserLocator(this.page.locator(selector).first());
  }

  async controls(): Promise<FormControl[]> {
    return this.page.evaluate(() => {
      const controls = Array.from(document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>(
        "input, textarea, select",
      ));
      return controls
        .filter((control) => !control.disabled)
        .map((control, index) => {
          const fieldId = control.dataset.blueyFieldId || `field-${index}`;
          control.dataset.blueyFieldId = fieldId;
          const byFor = control.id ? document.querySelector<HTMLLabelElement>(`label[for="${CSS.escape(control.id)}"]`) : null;
          const wrapping = control.closest("label");
          const ariaLabelledBy = control.getAttribute("aria-labelledby")
            ?.split(/\s+/)
            .map((id) => document.getElementById(id)?.textContent || "")
            .join(" ");
          const nearbyLegend = control.closest("fieldset")?.querySelector("legend")?.textContent || "";
          const label = (byFor?.textContent || wrapping?.textContent || control.getAttribute("aria-label")
            || ariaLabelledBy || nearbyLegend || "").replace(/\s+/g, " ").trim();
          const rawType = control instanceof HTMLSelectElement
            ? "select"
            : control instanceof HTMLTextAreaElement
              ? "textarea"
              : control.type.toLowerCase();
          const supported = ["text", "email", "tel", "url", "select", "textarea", "checkbox", "radio", "file", "hidden"];
          const kind = supported.includes(rawType) ? rawType : "other";
          return {
            selector: `[data-bluey-field-id="${fieldId}"]`,
            kind,
            label,
            name: control.getAttribute("name") || control.id || "",
            placeholder: control.getAttribute("placeholder") || "",
            required: control.required || control.getAttribute("aria-required") === "true",
            value: control instanceof HTMLInputElement && control.type === "file"
              ? Array.from(control.files || []).map((file) => file.name).join(", ")
              : control.value,
            checked: control instanceof HTMLInputElement && ["checkbox", "radio"].includes(control.type)
              ? control.checked
              : undefined,
            options: control instanceof HTMLSelectElement
              ? Array.from(control.options).map((option) => ({ label: option.text, value: option.value }))
              : undefined,
          };
        });
    }) as Promise<FormControl[]>;
  }

  bodyText(): Promise<string> {
    return this.page.locator("body").innerText().catch(() => "");
  }

  async waitForSettled(): Promise<void> {
    await this.page.waitForLoadState("domcontentloaded", { timeout: 20_000 }).catch(() => undefined);
    await this.page.waitForLoadState("networkidle", { timeout: 3_000 }).catch(() => undefined);
  }

  async screenshot(options?: { fullPage?: boolean }): Promise<Uint8Array> {
    return this.page.screenshot({ fullPage: options?.fullPage ?? true });
  }
}

class PlaywrightBrowserLocator implements BrowserLocator {
  constructor(private readonly locator: Locator) {}

  count(): Promise<number> {
    return this.locator.count();
  }

  fill(value: string): Promise<void> {
    return this.locator.fill(value);
  }

  click(): Promise<void> {
    return this.locator.click();
  }

  textContent(): Promise<string | null> {
    return this.locator.textContent();
  }

  getAttribute(name: string): Promise<string | null> {
    return this.locator.getAttribute(name);
  }

  isVisible(): Promise<boolean> {
    return this.locator.isVisible();
  }

  async selectOption(value: string): Promise<void> {
    await this.locator.selectOption(value);
  }

  async setChecked(checked: boolean): Promise<void> {
    await this.locator.setChecked(checked);
  }

  async setInputFiles(paths: string[]): Promise<void> {
    await this.locator.setInputFiles(paths);
  }
}

export function formControlKind(value: string): FormControlKind {
  return ["text", "email", "tel", "url", "textarea", "select", "checkbox", "radio", "file", "hidden"]
    .includes(value) ? value as FormControlKind : "other";
}
