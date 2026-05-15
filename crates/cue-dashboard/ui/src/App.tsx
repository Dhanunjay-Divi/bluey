import { useState, useEffect } from "react";
import { HashRouter, Routes, Route } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { DashboardLayout } from "./components/DashboardLayout";
import { Chats } from "./pages/Chats";
import { SessionDetail } from "./pages/SessionDetail";
import { Placeholder } from "./pages/Placeholder";
import { Settings } from "./pages/Settings";
import { Onboarding } from "./pages/Onboarding";

function App() {
  const [ready, setReady] = useState(false);
  const [needsOnboarding, setNeedsOnboarding] = useState(false);

  useEffect(() => {
    invoke<Record<string, string>>("load_settings").then((s) => {
      setNeedsOnboarding(s.onboarding_complete !== "true");
      setReady(true);
    });
  }, []);

  if (!ready) return null;

  if (needsOnboarding) {
    return <Onboarding onComplete={() => setNeedsOnboarding(false)} />;
  }

  return (
    <HashRouter>
      <Routes>
        <Route element={<DashboardLayout />}>
          <Route index element={<Placeholder name="Home" />} />
          <Route path="chats" element={<Chats />} />
          <Route path="session/:id" element={<SessionDetail />} />
          <Route path="prompts" element={<Placeholder name="Prompts" />} />
          <Route path="shortcuts" element={<Placeholder name="Shortcuts" />} />
          <Route path="settings" element={<Settings />} />
          <Route path="responses" element={<Placeholder name="Responses" />} />
          <Route path="screenshot" element={<Placeholder name="Screenshot" />} />
          <Route path="audio" element={<Placeholder name="Audio" />} />
          <Route path="dev" element={<Placeholder name="Dev Tools" />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}

export default App;
