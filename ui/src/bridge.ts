import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { mockCall } from "./mock";
import type { Preset, PresetPreview, ProcessResult, State } from "./types";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) return invoke<T>(command, args);
  return mockCall<T>(command, args);
}

/**
 * One function per host command. The UI never asks which runtime it is in;
 * the browser fallback lives behind this module.
 */
export const bridge = {
  getState: () => call<State>("get_state"),

  onState(handler: (state: State) => void): Promise<UnlistenFn> {
    if (!inTauri) return Promise.resolve(() => {});
    return listen<State>("state", (event) => handler(event.payload));
  },

  addLaser: (name: string, watts: number) => call<State>("add_laser", { name, watts }),
  renameLaser: (id: string, name: string, watts: number) => call<State>("rename_laser", { id, name, watts }),
  removeLaser: (id: string) => call<State>("remove_laser", { id }),
  selectLaser: (id: string) => call<State>("select_laser", { id }),

  selectPreset: (id: string) => call<State>("select_preset", { id }),
  addPreset: (name: string, laserId: string) => call<State>("add_preset", { name, laserId }),
  savePreset: (preset: Preset) => call<State>("save_preset", { preset }),
  removePreset: (id: string) => call<State>("remove_preset", { id }),
  importScan: (id: string, path: string) => call<State>("import_scan", { id, path }),
  exportGauge: (id: string, path: string) => call<State>("export_gauge", { id, path }),
  presetPreview: (id: string) => call<PresetPreview>("preset_preview", { id }),
  openScanArchive: () => call<State>("open_scan_archive"),
  clearScanArchive: () => call<State>("clear_scan_archive"),

  processPhoto: (tab: string, path: string) => call<ProcessResult>("process_photo", { tab, path }),
  saveEngraving: (tab: string, path: string) => call<State>("save_engraving", { tab, path }),
  saveSimulation: (tab: string, path: string) => call<State>("save_simulation", { tab, path }),
  saveReport: (tab: string, path: string) => call<State>("save_report", { tab, path }),

  pickOpenFile: (title: string, extensions: string[]) =>
    call<string | null>("pick_open_file", { title, extensions }),
  pickSaveFile: (title: string, fileName: string, extension: string) =>
    call<string | null>("pick_save_file", { title, fileName, extension }),

  minimizeWindow: () => call<void>("minimize_window"),
  toggleMaximizeWindow: () => call<void>("toggle_maximize_window"),
  closeWindow: () => call<void>("close_window"),
};

export const errorText = (error: unknown): string => {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "The host reported an unknown error.";
};
