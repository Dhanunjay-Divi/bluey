import { Check } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { type DisguiseMode, getDisguise, setDisguise } from "../lib/disguise";

interface SettingsState {
  stt_provider: string;
  "audio.mic_device": string;
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
    "audio.mic_device": "",
    language_hint: "en",
  });
  const [sttKeyDisplay, setSttKeyDisplay] = useState("");
  const [saving, setSaving] = useState(false);
  const [disguise, setDisguiseState] = useState<DisguiseMode>("none");
  const [passthrough, setPassthrough] = useState(true);
  const [keybinds, setKeybinds] = useState<{action: string; accelerator: string}[]>([]);
  const [recordingAction, setRecordingAction] = useState<string | null>(null);

  useEffect(() => {
    invoke<Record<string, string>>("load_settings")
      .then((s) => {
        setSettings({
          stt_provider: s.stt_provider ?? "deepgram",
          "audio.mic_device": s["audio.mic_device"] ?? "",
          language_hint: s.language_hint ?? "en",
        });
        // Load masked API key from keyring for the current provider
        const provider = s.stt_provider ?? "deepgram";
        return invoke<string | null>("load_stt_api_key", { provider });
      })
      .then((masked) => {
        if (masked) setSttKeyDisplay(masked);
      })
      .catch((e) => console.warn("load_settings/key failed:", e));

    getDisguise()
      .then(setDisguiseState)
      .catch((e) => console.warn("get_disguise failed:", e));

    invoke<boolean>("get_mouse_passthrough")
      .then(setPassthrough)
      .catch((e) => console.warn("get_mouse_passthrough failed:", e));

    invoke<{action: string; accelerator: string}[]>("list_keybinds")
      .then(setKeybinds)
      .catch((e) => console.warn("list_keybinds failed:", e));
  }, []);

  const save = () => {
    setSaving(true);
    // Save non-secret settings only (api_key is stripped server-side too)
    invoke("save_settings", { settings })
      .catch((e) => console.warn("save_settings failed:", e))
      .finally(() => setSaving(false));
  };

  const handleSttKeyChange = (key: string) => {
    setSttKeyDisplay(key);
  };

  const handleSttKeyCommit = (key: string) => {
    if (!key || key.startsWith("****")) return;
    invoke("save_stt_api_key", { provider: settings.stt_provider, key })
      .then(() => invoke<string | null>("load_stt_api_key", { provider: settings.stt_provider }))
      .then((masked) => { if (masked) setSttKeyDisplay(masked); })
      .catch((e) => console.warn("save_stt_api_key failed:", e));
  };

  const handleProviderChange = (provider: string) => {
    setSettings({ ...settings, stt_provider: provider });
    setSttKeyDisplay("");
    invoke<string | null>("load_stt_api_key", { provider })
      .then((masked) => { if (masked) setSttKeyDisplay(masked); })
      .catch(() => {});
  };

  const handleDisguiseChange = (mode: DisguiseMode) => {
    setDisguiseState(mode);
    setDisguise(mode).catch((e) =>
      console.warn("set_disguise failed:", e)
    );
  };

  const handlePassthroughChange = (enabled: boolean) => {
    setPassthrough(enabled);
    invoke("set_mouse_passthrough", { enabled }).catch((e) =>
      console.warn("set_mouse_passthrough failed:", e)
    );
  };

  const handleSetKeybind = (action: string, accelerator: string) => {
    invoke("set_keybind", { action, accelerator })
      .then(() => {
        setKeybinds((prev) =>
          prev.map((k) => (k.action === action ? { ...k, accelerator } : k))
        );
      })
      .catch((e) => console.warn("set_keybind failed:", e));
    setRecordingAction(null);
  };

  const handleResetKeybinds = () => {
    invoke("reset_keybinds")
      .then(() => invoke<{action: string; accelerator: string}[]>("list_keybinds"))
      .then(setKeybinds)
      .catch((e) => console.warn("reset_keybinds failed:", e));
  };

  const labels = isMac() ? MAC_LABELS : WIN_LABELS;

  return (
    <div className="p-6 max-w-xl space-y-6">
      <DisguiseSection />
      <h1 className="text-2xl font-bold">Settings</h1>

      <label className="block">
        <span className="text-sm font-medium">STT Provider</span>
        <select
          className="mt-1 block w-full rounded border border-neutral-600 bg-neutral-800 px-3 py-2"
          value={settings.stt_provider}
          onChange={(e) => handleProviderChange(e.target.value)}
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
          value={sttKeyDisplay}
          onChange={(e) => handleSttKeyChange(e.target.value)}
          onBlur={(e) => handleSttKeyCommit(e.target.value)}
          placeholder="••••••••"
        />
        <span className="text-xs text-zinc-500">Stored securely in system keyring</span>
      </label>

      <label className="block">
        <span className="text-sm font-medium">Microphone Device</span>
        <input
          type="text"
          className="mt-1 block w-full rounded border border-neutral-600 bg-neutral-800 px-3 py-2"
          value={settings["audio.mic_device"]}
          onChange={(e) =>
            setSettings({ ...settings, "audio.mic_device": e.target.value })
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
      <section className="space-y-3">
        <h2 className="text-lg font-semibold">Overlay</h2>
        <label className="flex items-center gap-3">
          <input
            type="checkbox"
            checked={passthrough}
            onChange={(e) => handlePassthroughChange(e.target.checked)}
            className="h-4 w-4 rounded border-neutral-600 bg-neutral-800"
          />
          <span className="text-sm">Click-through (mouse passthrough)</span>
        </label>
      </section>

      <hr className="border-neutral-700" />

      <section className="space-y-3">
        <h2 className="text-lg font-semibold">Keybinds</h2>
        {keybinds.map((kb) => (
          <div key={kb.action} className="flex items-center gap-3">
            <span className="w-40 text-sm">{kb.action.replace(/_/g, " ")}</span>
            {recordingAction === kb.action ? (
              <input
                autoFocus
                className="flex-1 rounded border border-blue-500 bg-neutral-800 px-3 py-1 text-sm"
                placeholder="Press keys..."
                onKeyDown={(e) => {
                  e.preventDefault();
                  const parts: string[] = [];
                  if (e.metaKey || e.ctrlKey) parts.push("CmdOrCtrl");
                  if (e.shiftKey) parts.push("Shift");
                  if (e.altKey) parts.push("Alt");
                  const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
                  if (!["Control", "Shift", "Alt", "Meta"].includes(e.key)) {
                    parts.push(key);
                    handleSetKeybind(kb.action, parts.join("+"));
                  }
                }}
                onBlur={() => setRecordingAction(null)}
              />
            ) : (
              <button
                onClick={() => setRecordingAction(kb.action)}
                className="flex-1 rounded border border-neutral-600 bg-neutral-800 px-3 py-1 text-left text-sm hover:border-neutral-400"
              >
                {kb.accelerator}
              </button>
            )}
          </div>
        ))}
        <button
          onClick={handleResetKeybinds}
          className="rounded border border-neutral-600 px-3 py-1 text-sm hover:bg-neutral-700"
        >
          Reset to Defaults
        </button>
      </section>
    </div>
  );
}

// ===== Phase 3 Round 9: AI Provider Settings =====
// LLM API keys are stored via keyring (save_llm_api_key command).
// Ollama does not require an API key (local inference).

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


// ─── Codex Stage 18 commit 7: Disguise picker section ────────────────────

const DISGUISE_OPTIONS = [
  { value: "none", label: "Off (visible as Bluey)", desc: "Bluey shows up as itself in your menu bar." },
  { value: "activity", label: "Activity Monitor", desc: "Recommended. Looks like the system process viewer." },
  { value: "terminal", label: "Terminal", desc: "Looks like an open terminal window." },
  { value: "settings", label: "System Settings", desc: "Looks like an open settings pane." },
];

function DisguiseSection() {
  const [mode, setMode] = useState<string>("activity");

  useEffect(() => {
    invoke<string>("get_disguise").then((m) => setMode(m)).catch(() => {});
  }, []);

  async function update(next: string) {
    setMode(next);
    try {
      await invoke("set_disguise", { mode: next });
    } catch (e) {
      console.warn("set_disguise failed", e);
    }
  }

  const current = DISGUISE_OPTIONS.find((o) => o.value === mode) ?? DISGUISE_OPTIONS[0];

  return (
    <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-5 space-y-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 className="font-semibold text-zinc-100">Disguise</h3>
          <p className="text-xs text-zinc-400 mt-1 leading-relaxed">
            How Bluey appears in your menu bar and to screen-shares. We
            recommend leaving this on Activity Monitor unless you have a
            specific reason.{" "}
            <a
              href="https://bluey.dev/docs/disguise"
              target="_blank"
              rel="noreferrer"
              className="text-blue-400 hover:text-blue-300 underline"
            >
              Why?
            </a>
          </p>
        </div>
      </div>

      <div className="space-y-2">
        {DISGUISE_OPTIONS.map((opt) => {
          const active = opt.value === mode;
          return (
            <button
              key={opt.value}
              onClick={() => update(opt.value)}
              className={
                "w-full text-left rounded-lg border p-3 transition-colors " +
                (active
                  ? "bg-blue-500/10 border-blue-500/40"
                  : "bg-zinc-950 border-zinc-800 hover:border-zinc-700")
              }
            >
              <div className="flex items-center justify-between">
                <span className={"text-sm font-medium " + (active ? "text-blue-300" : "text-zinc-200")}>
                  {opt.label}
                </span>
                {active && <Check className="h-4 w-4 text-blue-400" />}
              </div>
              <p className="text-xs text-zinc-500 mt-1">{opt.desc}</p>
            </button>
          );
        })}
      </div>

      <p className="text-xs text-zinc-500">
        Currently disguised as <span className="text-zinc-300">{current.label}</span>.
      </p>
    </div>
  );
}
