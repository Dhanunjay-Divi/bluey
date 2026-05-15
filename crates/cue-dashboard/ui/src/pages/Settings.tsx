import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Settings {
  stt_provider: string;
  audio_device: string;
  system_audio_continuous: string;
  system_audio_stt: string;
  language_hint: string;
}

const DEFAULT_SETTINGS: Settings = {
  stt_provider: "deepgram",
  audio_device: "",
  system_audio_continuous: "0",
  system_audio_stt: "0",
  language_hint: "",
};

export function Settings() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [devices, setDevices] = useState<string[]>([]);
  const [apiKey, setApiKey] = useState("");
  const [maskedKey, setMaskedKey] = useState<string | null>(null);

  useEffect(() => {
    invoke<Record<string, string>>("load_settings").then((s) => {
      setSettings({ ...DEFAULT_SETTINGS, ...s });
    });
    invoke<string[]>("list_audio_devices").then(setDevices);
    invoke<string | null>("load_stt_api_key", { provider: "deepgram" }).then(
      setMaskedKey
    );
  }, []);

  const saveKey = async () => {
    if (!apiKey) return;
    await invoke("save_stt_api_key", {
      provider: settings.stt_provider,
      key: apiKey,
    });
    setMaskedKey(apiKey.length <= 4 ? "****" : `****${apiKey.slice(-4)}`);
    setApiKey("");
  };

  const update = (key: keyof Settings, value: string) => {
    const next = { ...settings, [key]: value };
    setSettings(next);
    invoke("save_settings", { settings: next });
  };

  return (
    <div className="flex flex-col gap-6 p-6 overflow-y-auto h-full">
      <h2 className="text-xl font-bold text-zinc-100">Settings</h2>

      <section className="flex flex-col gap-2">
        <label className="text-sm font-medium text-zinc-300">STT Provider</label>
        <select
          className="rounded bg-zinc-800 px-3 py-2 text-zinc-100 border border-zinc-700"
          value={settings.stt_provider}
          onChange={(e) => update("stt_provider", e.target.value)}
        >
          <option value="deepgram">Deepgram</option>
          <option value="echo">Echo (fallback)</option>
          <option value="openai" disabled>OpenAI (coming soon)</option>
          <option value="assemblyai" disabled>AssemblyAI (coming soon)</option>
        </select>
      </section>

      <section className="flex flex-col gap-2">
        <label className="text-sm font-medium text-zinc-300">API Key</label>
        {maskedKey && <p className="text-xs text-zinc-500">Current: {maskedKey}</p>}
        <div className="flex gap-2">
          <input
            type="password"
            className="flex-1 rounded bg-zinc-800 px-3 py-2 text-zinc-100 border border-zinc-700"
            placeholder="Enter API key..."
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
          />
          <button
            className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
            onClick={saveKey}
            disabled={!apiKey}
          >
            Save
          </button>
        </div>
      </section>

      <section className="flex flex-col gap-2">
        <label className="text-sm font-medium text-zinc-300">Audio Device</label>
        <select
          className="rounded bg-zinc-800 px-3 py-2 text-zinc-100 border border-zinc-700"
          value={settings.audio_device}
          onChange={(e) => update("audio_device", e.target.value)}
        >
          <option value="">System Default</option>
          {devices.map((d) => (
            <option key={d} value={d}>{d}</option>
          ))}
        </select>
      </section>

      <section className="flex items-center justify-between">
        <span className="text-sm text-zinc-300">System Audio Capture (continuous)</span>
        <input
          type="checkbox"
          className="h-4 w-4"
          checked={settings.system_audio_continuous === "1"}
          onChange={(e) => update("system_audio_continuous", e.target.checked ? "1" : "0")}
        />
      </section>

      <section className="flex items-center justify-between">
        <span className="text-sm text-zinc-300">STT for System Audio</span>
        <input
          type="checkbox"
          className="h-4 w-4"
          checked={settings.system_audio_stt === "1"}
          onChange={(e) => update("system_audio_stt", e.target.checked ? "1" : "0")}
        />
      </section>

      <section className="flex flex-col gap-2">
        <label className="text-sm font-medium text-zinc-300">Language Hint (BCP-47)</label>
        <input
          type="text"
          className="rounded bg-zinc-800 px-3 py-2 text-zinc-100 border border-zinc-700"
          placeholder="e.g. en-US (empty = auto-detect)"
          value={settings.language_hint}
          onChange={(e) => update("language_hint", e.target.value)}
        />
      </section>
    </div>
  );
}
