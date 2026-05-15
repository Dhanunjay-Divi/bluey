import { useEffect, useState } from "react";
import { HashRouter, Routes, Route, useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DashboardLayout } from "./components/DashboardLayout";
import { Chats } from "./pages/Chats";
import { SessionDetail } from "./pages/SessionDetail";
import { Placeholder } from "./pages/Placeholder";
import { Search } from "./routes/Search";
import { UpdateToast } from "./components/UpdateToast";
import { Onboarding } from "./pages/Onboarding";

/** Listens for tray "navigate_to" events and routes accordingly. */
function NavigateListener() {
  const navigate = useNavigate();
  useEffect(() => {
    const unlisten = listen<string>("navigate_to", (event) => {
      navigate(event.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [navigate]);
  return null;
}

/** Subscribe to hotkey/tray events and forward them to daemon via Tauri commands. */
function HotkeyListener() {
  useEffect(() => {
    const unlisteners = [
      listen("hotkey_toggle_listening", () => {
        invoke("daemon_toggle_listening").catch((e) =>
          console.warn("daemon_toggle_listening failed:", e)
        );
      }),
      listen("hotkey_push_to_talk", () => {
        invoke("daemon_set_push_to_talk").catch((e) =>
          console.warn("daemon_set_push_to_talk failed:", e)
        );
      }),
      listen("hotkey_toggle_overlay", () => {
        invoke("daemon_toggle_overlay").catch((e) =>
          console.warn("daemon_toggle_overlay failed:", e)
        );
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()));
    };
  }, []);
  return null;
}

function App() {
  const [ready, setReady] = useState(false);
  const [showOnboarding, setShowOnboarding] = useState(false);

  useEffect(() => {
    invoke<Record<string, string>>("load_settings")
      .then((settings) => {
        if (!settings?.onboarding_complete || settings.onboarding_complete !== "true") {
          setShowOnboarding(true);
        }
      })
      .catch(() => {
        setShowOnboarding(true);
      })
      .finally(() => setReady(true));
  }, []);

  if (!ready) return null;

  if (showOnboarding) {
    return <Onboarding onComplete={() => setShowOnboarding(false)} />;
  }

  return (
    <HashRouter>
      <NavigateListener />
      <HotkeyListener />
      <Routes>
        <Route element={<DashboardLayout />}>
          <Route index element={<Placeholder name="Home" />} />
          <Route path="chats" element={<Chats />} />
          <Route path="session/:id" element={<SessionDetail />} />
          <Route path="prompts" element={<Placeholder name="Prompts" />} />
          <Route path="shortcuts" element={<Placeholder name="Shortcuts" />} />
          <Route path="settings" element={<Placeholder name="Settings" />} />
          <Route path="responses" element={<Placeholder name="Responses" />} />
          <Route path="screenshot" element={<Placeholder name="Screenshot" />} />
          <Route path="audio" element={<Placeholder name="Audio" />} />
          <Route path="search" element={<Search />} />
          <Route path="dev" element={<Placeholder name="Dev Tools" />} />
        </Route>
      </Routes>
      <UpdateToast />
    </HashRouter>
  );
}

export default App;
