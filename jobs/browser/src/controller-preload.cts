import { contextBridge, ipcRenderer } from "electron";
import type {
  BlueyBrowserBridge,
  ControllerCommand,
  ControllerViewState,
} from "./controller-contract.js";

const IPC_GET_STATE = "bluey-browser:get-state";
const IPC_STATE = "bluey-browser:state";
const IPC_COMMAND = "bluey-browser:command";
const IPC_BACKGROUND = "bluey-browser:set-background";

const COMMANDS = new Set<ControllerCommand>([
  "primary",
  "open-browser",
  "toggle-pause",
  "stop",
  "open-jobs",
]);

const bridge: BlueyBrowserBridge = Object.freeze({
  getState: () => ipcRenderer.invoke(IPC_GET_STATE) as Promise<ControllerViewState>,
  command: (command: ControllerCommand) => {
    if (COMMANDS.has(command)) ipcRenderer.send(IPC_COMMAND, command);
  },
  setBackgroundEnabled: (enabled: boolean) => {
    if (typeof enabled !== "boolean") return Promise.reject(new Error("Invalid background preference"));
    return ipcRenderer.invoke(IPC_BACKGROUND, enabled) as Promise<boolean>;
  },
  onState: (listener: (state: ControllerViewState) => void) => {
    if (typeof listener !== "function") return () => undefined;
    const wrapped = (_event: Electron.IpcRendererEvent, state: ControllerViewState) => listener(state);
    ipcRenderer.on(IPC_STATE, wrapped);
    return () => ipcRenderer.removeListener(IPC_STATE, wrapped);
  },
});

contextBridge.exposeInMainWorld("blueyBrowser", bridge);
