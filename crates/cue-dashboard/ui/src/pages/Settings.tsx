import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SettingsState {
  stt_provider: string;
  stt_api_key: string;
  mic_device: string;
  language_hint: string;
}

const STT_PROVIDERS = ["deepgram", "echo", "openai", "local_whisper"];

export function Settings() {
  const [settings, setSettings] = useState<SettingsState>({
    stt_provider: "deepgram",
    stt_api_key: "",
    mic_device: "",
    language_hint: "en",
  });
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    invoke<Record<string, string>>("load_settings")
      .then((s) => {
        setSettings({
          stt_provider: s.stt_provider ?? "deepgram",
          stt_api_key: s.stt_api_key ?? "",
          mic_device: s.mic_device ?? "",
          language_hint: s.language_hint ?? "en",
        });
      })
      .catch((e) => console.warn("load_settings failed:", e));
  }, []);

  const save = () => {
    setSaving(true);
    invoke("save_settings", { settings })
      .catch((e) => console.warn("save_settings failed:", e))
      .finally(() => setSaving(false));
  };

  return (
    <div className="p-6 max-w-xl space-y-6">
      <h1 className="text-2xl font-bold">Settings</h1>

      <label className="block">
        <span className="text-sm font-medium">STT Provider</span>
        <select
          className="mt-1 block w-full rounded border border-neutral-600 bg-neutral-800 px-3 py-2"
          value={settings.stt_provider}
          onChange={(e) =>
            setSettings({ ...settings, stt_provider: e.target.value })
          }
        >
          {STT_PROVIDERS.map((p) => (
            <option key={p} value={p}>
              {p.replace("_", " ").replace(/\b\w/g, (c) => c.toUpperCase())}
            </option>
          ))}
        </select>
      </label>

      <label className="block">
        <span className="text-sm font-medium">API Key</span>
        <input
          type="password"
          className="mt-1 block w-full rounded border border-neutral-600 bg-neutral-800 px-3 py-2"
          value={settings.stt_api_key}
          onChange={(e) =>
            setSettings({ ...settings, stt_api_key: e.target.value })
          }
          placeholder="••••••••"
        />
      </label>

      <label className="block">
        <span className="text-sm font-medium">Microphone Device</span>
        <input
          type="text"
          className="mt-1 block w-full rounded border border-neutral-600 bg-neutral-800 px-3 py-2"
          value={settings.mic_device}
          onChange={(e) =>
            setSettings({ ...settings, mic_device: e.target.value })
          }
          placeholder="default"
        />
      </label>

      <label className="block">
        <span className="text-sm font-medium">Language Hint</span>
        <input
          type="text"
          className="mt-1 block w-full rounded border border-neutral-600 bg-neutral-800 px-3 py-2"
          value={settings.language_hint}
          onChange={(e) =>
            setSettings({ ...settings, language_hint: e.target.value })
          }
          placeholder="en"
        />
      </label>

      <button
        onClick={save}
        disabled={saving}
        className="rounded bg-blue-600 px-4 py-2 font-medium text-white hover:bg-blue-500 disabled:opacity-50"
      >
        {saving ? "Saving…" : "Save"}
      </button>
    </div>
  );
}
