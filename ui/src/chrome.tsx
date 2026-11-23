import { For } from "solid-js";
import { bridge } from "./bridge";
import type { Area } from "./types";

const AREAS: { id: Area; label: string }[] = [
  { id: "lasers", label: "Lasers" },
  { id: "presets", label: "Presets" },
  { id: "engrave", label: "Engrave" },
];

const ignore = () => {
  /* window commands only fail if the host is gone */
};

type Props = {
  area: Area;
  onArea: (area: Area) => void;
};

/** Undecorated window chrome: drag region, area tabs, window controls. */
export function Chrome(props: Props) {
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
