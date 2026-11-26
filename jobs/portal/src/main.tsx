import { Component, StrictMode, Suspense, lazy, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { AuthGate } from "./components/AuthGate";
import { initialTheme, loginUrl, startupMode } from "./auth";
import { canonicalLegacyAutomationUrl } from "./lib/portal-navigation";
import "./styles.css";

const canonicalUrl = canonicalLegacyAutomationUrl(
  window.location.pathname,
  window.location.search,
);
if (canonicalUrl) window.history.replaceState(window.history.state, "", canonicalUrl);

document.documentElement.dataset.theme = initialTheme();

const root = document.getElementById("root");
if (!root) throw new Error("Bluey Jobs root is unavailable.");
const mode = startupMode(
  window.location.search,
  root.dataset.blueyJobsPrerendered === "true",
);
const App = mode === "app" ? lazy(() => import("./App")) : null;

class WorkspaceLoadBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <main className="jobs-entry">
        <h1>Bluey Jobs could not finish loading.</h1>
        <p>Retry the workspace or sign in again to continue safely.</p>
        <p><a className="button primary" href={loginUrl()}>Sign in to Bluey</a></p>
        <button className="button secondary" onClick={() => window.location.reload()}>Retry loading</button>
      </main>
    );
  }
}

if (mode !== "none") createRoot(root).render(
  <StrictMode>
    <BrowserRouter basename="/jobs">
      {App ? (
        <WorkspaceLoadBoundary>
          <Suspense fallback={<main className="jobs-entry"><p>Loading Bluey Jobs…</p></main>}>
            <App />
          </Suspense>
        </WorkspaceLoadBoundary>
      ) : <AuthGate />}
    </BrowserRouter>
  </StrictMode>,
);
