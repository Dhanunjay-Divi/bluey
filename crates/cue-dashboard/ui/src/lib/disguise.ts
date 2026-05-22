import { invoke } from "./tauri";

export type DisguiseMode = "none" | "terminal" | "settings" | "activity";

export async function setDisguise(mode: DisguiseMode): Promise<void> {
  return invoke("set_disguise", { mode });
}

export async function getDisguise(): Promise<DisguiseMode> {
  return invoke("get_disguise");
}
