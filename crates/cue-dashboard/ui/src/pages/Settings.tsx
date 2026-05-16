import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { type DisguiseMode, getDisguise, setDisguise } from "../lib/disguise";

interface SettingsState {
  stt_provider: string;
  stt_api_key: string;
  mic_device: string;
  language_hint: string;
}

const STT_PROVIDERS = ["deepgram", "echo", "openai", "local_whisper"];

const MAC_LABELS: Record<DisguiseMode, string> = {
  none: "None",
  terminal: "Terminal",
  settings: "System Settings",
  activity: "Activity Monitor",
};

const WIN_LABELS: Record<DisguiseMode, string> = {
  none: "None",
  terminal: "Command Prompt",
  settings: "Settings",
  activity: "Task Manager",
};

function isMac(): boolean {
  return navigator.platform.toLowerCase().includes("mac");
}

export function Settings() {
  const [settings, setSettings] = useState<SettingsState>({
    stt_provider: "deepgram",
    stt_api_key: "",
    mic_device: "",
    language_hint: "en",
  });
  const [saving, setSaving] = useState(false);
  const [disguise, setDisguiseState] = useState<DisguiseMode>("none");

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

    getDisguise()
      .then(setDisguiseState)
      .catch((e) => console.warn("get_disguise failed:", e));
  }, []);

  const save = () => {
    setSaving(true);
    invoke("save_settings", { settings })
      .catch((e) => console.warn("save_settings failed:", e))
      .finally(() => setSaving(false));
  };

  const handleDisguiseChange = (mode: DisguiseMode) => {
    setDisguiseState(mode);
    setDisguise(mode).catch((e) =>
      console.warn("set_disguise failed:", e)
    );
  };

  const labels = isMac() ? MAC_LABELS : WIN_LABELS;

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

      <hr className="border-neutral-700" />

      <section className="space-y-3">
        <h2 className="text-lg font-semibold">Stealth &amp; Disguise</h2>
        <label className="block">
          <span className="text-sm font-medium">Disguise Mode</span>
          <select
            className="mt-1 block w-full rounded border border-neutral-600 bg-neutral-800 px-3 py-2"
            value={disguise}
            onChange={(e) =>
              handleDisguiseChange(e.target.value as DisguiseMode)
            }
          >
            {(Object.keys(labels) as DisguiseMode[]).map((mode) => (
              <option key={mode} value={mode}>
                {labels[mode]}
              </option>
            ))}
          </select>
        </label>
      </section>

      <hr className="border-neutral-700" />

      <AiProviderSettings />
    </div>
  );
}

// ===== Phase 3 Round 9: AI Provider Settings =====


const LLM_PROVIDERS = ["anthropic", "openai", "ollama"];

export function AiProviderSettings() {
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [chain, setChain] = useState<string[]>(LLM_PROVIDERS);

  useEffect(() => {
    invoke<Record<string, string>>("load_settings").then((s) => {
      if (s["llm.chain"]) {
        setChain(s["llm.chain"].split(",").filter(Boolean));
      }
    });
  }, []);

  const saveKey = useCallback((provider: string, key: string) => {
    setKeys((prev) => ({ ...prev, [provider]: key }));
    invoke("save_llm_api_key", { provider, key }).catch((e) =>
      console.warn("save_llm_api_key failed:", e)
    );
  }, []);

  const saveChain = useCallback((newChain: string[]) => {
    setChain(newChain);
    invoke("set_llm_chain", { providers: newChain }).catch((e) =>
      console.warn("set_llm_chain failed:", e)
    );
  }, []);

  const moveUp = (idx: number) => {
    if (idx === 0) return;
    const next = [...chain];
    [next[idx - 1], next[idx]] = [next[idx], next[idx - 1]];
    saveChain(next);
  };

  const moveDown = (idx: number) => {
    if (idx >= chain.length - 1) return;
    const next = [...chain];
    [next[idx], next[idx + 1]] = [next[idx + 1], next[idx]];
    saveChain(next);
  };

  return (
    <section className="space-y-3">
      <h2 className="text-lg font-semibold">AI Providers</h2>
      {LLM_PROVIDERS.map((provider) => (
        <div key={provider} className="flex items-center gap-2">
          <span className="w-24 text-sm capitalize">{provider}</span>
          {provider !== "ollama" && (
            <input
              type="password"
              className="flex-1 rounded border border-neutral-600 bg-neutral-800 px-2 py-1 text-sm"
              placeholder="API key"
              value={keys[provider] ?? ""}
              onChange={(e) => setKeys((p) => ({ ...p, [provider]: e.target.value }))}
              onBlur={(e) => { if (e.target.value) saveKey(provider, e.target.value); }}
            />
          )}
          {provider === "ollama" && (
            <span className="text-xs text-zinc-500">No key needed (local)</span>
          )}
        </div>
      ))}

      <h3 className="text-sm font-medium mt-4">Provider Chain (failover order)</h3>
      <div className="space-y-1">
        {chain.map((p, i) => (
          <div key={p} className="flex items-center gap-2 text-sm">
            <span className="w-24 capitalize">{p}</span>
            <button onClick={() => moveUp(i)} disabled={i === 0} className="text-xs text-zinc-400 hover:text-white disabled:opacity-30">↑</button>
            <button onClick={() => moveDown(i)} disabled={i === chain.length - 1} className="text-xs text-zinc-400 hover:text-white disabled:opacity-30">↓</button>
          </div>
        ))}
      </div>
    </section>
  );
}
