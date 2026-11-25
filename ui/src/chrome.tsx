import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { bridge } from "./bridge";
import type { Area, SpeedUnit } from "./types";

const AREAS: { id: Area; label: string }[] = [
  { id: "lasers", label: "Lasers" },
  { id: "presets", label: "Presets" },
  { id: "engrave", label: "Engrave" },
  { id: "faq", label: "FAQ" },
];

const ignore = () => {
  /* window commands only fail if the host is gone */
};

type Props = {
  area: Area;
  onArea: (area: Area) => void;
  speedUnit: SpeedUnit;
  onSpeedUnit: (unit: SpeedUnit) => void;
};

/** Undecorated window chrome: drag region, area tabs, window controls. */
export function Chrome(props: Props) {
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  let settings: HTMLDivElement | undefined;
  const outside = (event: PointerEvent) => {
    if (!settings?.contains(event.target as Node)) setSettingsOpen(false);
  };
  const escape = (event: KeyboardEvent) => {
    if (event.key === "Escape") setSettingsOpen(false);
  };
  onMount(() => {
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    onCleanup(() => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
    });
  });
  return (
    <header class="chrome" data-tauri-drag-region>
      <span class="wordmark" data-tauri-drag-region>
        engraving
      </span>

      <nav class="nav-areas" aria-label="Workspaces">
        <For each={AREAS}>
          {(entry) => (
            <button
              type="button"
              class={`nav-tab ${props.area === entry.id ? "active" : ""}`}
              aria-current={props.area === entry.id ? "page" : undefined}
              onClick={() => props.onArea(entry.id)}
            >
              {entry.label}
            </button>
          )}
        </For>
      </nav>

      <div class="chrome-space" data-tauri-drag-region />

      <div class="chrome-settings" ref={settings}>
        <button type="button" class="icon-btn settings-btn" aria-label="Settings" aria-expanded={settingsOpen()} onClick={() => setSettingsOpen((open) => !open)}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6.5 1.8h3l.4 1.5 1.2.7 1.5-.4 1.5 2.6-1.1 1.1v1.4l1.1 1.1-1.5 2.6-1.5-.4-1.2.7-.4 1.5h-3l-.4-1.5-1.2-.7-1.5.4-1.5-2.6L3 8.7V7.3L1.9 6.2l1.5-2.6 1.5.4 1.2-.7z" /><circle cx="8" cy="8" r="2.1" /></svg>
        </button>
        <Show when={settingsOpen()}>
          <div class="settings-popover" role="group" aria-label="Settings">
            <span class="section-label">Speed unit</span>
            <div class="speed-unit-options">
              <button type="button" class={props.speedUnit === "mm/s" ? "active" : ""} aria-pressed={props.speedUnit === "mm/s"} onClick={() => props.onSpeedUnit("mm/s")}>mm/s</button>
              <button type="button" class={props.speedUnit === "mm/min" ? "active" : ""} aria-pressed={props.speedUnit === "mm/min"} onClick={() => props.onSpeedUnit("mm/min")}>mm/min</button>
            </div>
          </div>
        </Show>
      </div>

      <div class="window-controls">
        <button
          type="button"
          class="win-btn"
          aria-label="Minimize window"
          title="Minimize"
          onClick={() => void bridge.minimizeWindow().catch(ignore)}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path d="M3 6h6" />
          </svg>
        </button>
        <button
          type="button"
          class="win-btn"
          aria-label="Maximize or restore window"
          title="Maximize"
          onClick={() => void bridge.toggleMaximizeWindow().catch(ignore)}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <rect x="3" y="3" width="6" height="6" />
          </svg>
        </button>
        <button
          type="button"
          class="win-btn close"
          aria-label="Close window"
          title="Close"
          onClick={() => void bridge.closeWindow().catch(ignore)}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path d="M3 3l6 6M9 3l-6 6" />
          </svg>
        </button>
      </div>
    </header>
  );
}
