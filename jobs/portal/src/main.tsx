import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { canonicalLegacyAutomationUrl } from "./lib/portal-navigation";
import "./styles.css";

const canonicalUrl = canonicalLegacyAutomationUrl(
  window.location.pathname,
  window.location.search,
);
if (canonicalUrl) window.history.replaceState(window.history.state, "", canonicalUrl);

const savedTheme = localStorage.getItem("bluey_jobs_theme");
const initialTheme = savedTheme === "light" || savedTheme === "dark"
  ? savedTheme
  : window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
document.documentElement.dataset.theme = initialTheme;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter basename="/jobs">
      <App />
    </BrowserRouter>
  </StrictMode>,
);
