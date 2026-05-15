import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Step = "welcome" | "apikey" | "mic" | "sysaudio" | "done";

export function Onboarding({ onComplete }: { onComplete: () => void }) {
  const [step, setStep] = useState<Step>("welcome");
  const [apiKey, setApiKey] = useState("");
  const [micOk, setMicOk] = useState(false);
  const [sysAudio, setSysAudio] = useState(false);

  const saveKeyAndNext = async () => {
    if (apiKey) {
      await invoke("save_stt_api_key", { provider: "deepgram", key: apiKey });
    }
    setStep("mic");
  };

  const testMic = async () => {
    const devices = await invoke<string[]>("list_audio_devices");
    setMicOk(devices.length > 0);
    setStep("sysaudio");
  };

  const finish = async () => {
    await invoke("save_settings", {
      settings: {
        onboarding_complete: "true",
        system_audio_continuous: sysAudio ? "1" : "0",
      },
    });
    setStep("done");
    onComplete();
  };

  return (
    <div className="flex h-full items-center justify-center bg-zinc-950">
      <div className="w-full max-w-md rounded-xl bg-zinc-900 p-8 shadow-xl border border-zinc-800">
        {step === "welcome" && (
          <div className="flex flex-col gap-4 text-center">
            <h1 className="text-2xl font-bold text-blue-400">
              Welcome to Bluey
            </h1>
            <p className="text-zinc-400">
              Let's get you set up in a few quick steps.
            </p>
            <button
              className="mt-4 rounded bg-blue-600 px-4 py-2 text-white hover:bg-blue-500"
              onClick={() => setStep("apikey")}
            >
              Get Started
            </button>
          </div>
        )}

        {step === "apikey" && (
          <div className="flex flex-col gap-4">
            <h2 className="text-lg font-semibold text-zinc-100">
              STT API Key
            </h2>
            <p className="text-sm text-zinc-400">
              Enter your Deepgram API key for speech-to-text.{" "}
              <a
                href="https://console.deepgram.com/signup"
                target="_blank"
                rel="noreferrer"
                className="text-blue-400 underline"
              >
                Sign up here
              </a>
            </p>
            <input
              type="password"
              className="rounded bg-zinc-800 px-3 py-2 text-zinc-100 border border-zinc-700"
              placeholder="dg-..."
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
            />
            <div className="flex gap-2">
              <button
                className="flex-1 rounded bg-blue-600 px-4 py-2 text-white hover:bg-blue-500"
                onClick={saveKeyAndNext}
              >
                {apiKey ? "Save & Continue" : "Skip"}
              </button>
            </div>
          </div>
        )}

        {step === "mic" && (
          <div className="flex flex-col gap-4">
            <h2 className="text-lg font-semibold text-zinc-100">
              Microphone Access
            </h2>
            <p className="text-sm text-zinc-400">
              Bluey needs microphone access for speech-to-text.
            </p>
            <button
              className="rounded bg-blue-600 px-4 py-2 text-white hover:bg-blue-500"
              onClick={testMic}
            >
              Test Microphone
            </button>
            {micOk && (
              <p className="text-sm text-green-400">✓ Microphone detected</p>
            )}
          </div>
        )}

        {step === "sysaudio" && (
          <div className="flex flex-col gap-4">
            <h2 className="text-lg font-semibold text-zinc-100">
              System Audio
            </h2>
            <p className="text-sm text-zinc-400">
              Enable system audio capture to transcribe meetings and media.
            </p>
            <label className="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                className="h-4 w-4"
                checked={sysAudio}
                onChange={(e) => setSysAudio(e.target.checked)}
              />
              <span className="text-sm text-zinc-300">
                Enable system audio capture
              </span>
            </label>
            <button
              className="rounded bg-blue-600 px-4 py-2 text-white hover:bg-blue-500"
              onClick={finish}
            >
              Finish Setup
            </button>
          </div>
        )}

        {step === "done" && (
          <div className="flex flex-col gap-4 text-center">
            <h2 className="text-lg font-semibold text-green-400">
              ✓ All Set!
            </h2>
            <p className="text-sm text-zinc-400">
              You can change these settings anytime from the Settings page.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
