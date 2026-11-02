import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { AuthGate } from "../src/components/AuthGate";

// AuthGate is the only content source; no workspace, account or browser globals.
export function renderPublicEntry(): string {
  return renderToStaticMarkup(createElement(AuthGate));
}
