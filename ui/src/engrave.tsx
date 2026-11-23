import { For, Show } from "solid-js";
import { Field } from "./fields";
import type { DocTab, Preset, ProcessResult } from "./types";

const baseName = (path: string) => path.split(/[\\/]/).pop() || path;

type StripProps = {
  tabs: DocTab[];
  activeId: string | null;
  busy: boolean;
  onActivate: (id: string) => void;
  onAdd: () => void;
  onClose: (id: string) => void;
};

/** Slim strip above the content surface: one tab per image. */
export function DocStrip(props: StripProps) {
  return (
    <div class="doc-strip">
      <div class="doc-tabs" role="tablist" aria-label="Image tabs">
        <For each={props.tabs}>
          {(tab) => (
            <div class={`doc-tab ${tab.id === props.activeId ? "active" : ""}`}>
              <button
                type="button"
                class="doc-tab-main"
                role="tab"
                aria-selected={tab.id === props.activeId}
                title={tab.title}
                disabled={props.busy}
                onClick={() => props.onActivate(tab.id)}
              >
                <Show when={tab.photoPath || tab.result}>
                  <span class="doc-mark" aria-hidden="true" />
                </Show>
                <span class="doc-name">{tab.title}</span>
              </button>
              <button
                type="button"
                class="doc-close"
                aria-label={`Close ${tab.title}`}
                title={`Close ${tab.title}`}
                disabled={props.busy}
                onClick={() => props.onClose(tab.id)}
              >
                <svg viewBox="0 0 12 12" aria-hidden="true">
                  <path d="M3.5 3.5l5 5M8.5 3.5l-5 5" />
                </svg>
              </button>
            </div>
          )}
        </For>
        <button
          type="button"
          class="icon-btn"
          title="New image tab"
          aria-label="New image tab"
          onClick={props.onAdd}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path d="M6 2v8M2 6h8" />
          </svg>
        </button>
      </div>
    </div>
  );
}

type PreviewProps = {
  title: string;
  tag: string;
  src?: string;
  empty: string;
};

function Preview(props: PreviewProps) {
  return (
    <figure class="preview">
      <figcaption class="preview-head">
        <span>{props.title}</span>
        <span class="preview-tag mono">{props.tag}</span>
      </figcaption>
      <div class="preview-body">
        <Show when={props.src} fallback={<p class="preview-empty">{props.empty}</p>}>
          {(src) => <img src={src()} alt={`${props.title} preview`} draggable={false} />}
        </Show>
      </div>
    </figure>
  );
}

type EngraveProps = {
  tab: DocTab | null;
  laserName: string | null;
  presets: Preset[];
  preset: Preset | null;
  presetSelected: boolean;
  result: ProcessResult | null;
  staleReason: string | null;
  busy: boolean;
  working: string | null;
  hasLasers: boolean;
  onPreset: (id: string) => void;
  onUsePreset: () => void;
  onChoose: () => void;
  onReprocess: () => void;
  onSave: (kind: "engraving" | "simulation" | "report") => void;
  onOpenPresets: () => void;
  onOpenLasers: () => void;
};

export function EngraveArea(props: EngraveProps) {
  const tab = () => props.tab;
  const gauge = () => (props.preset ? props.preset.calibration : null);
  const canProcess = () =>
    !props.busy && !!tab()?.presetId && props.preset?.has_scan === true && props.presetSelected;

  const canSave = () => !!props.result && !props.busy && props.presetSelected;

  const emptyText = () =>
    props.staleReason ? "Waiting for a processed image." : "Choose an image to engrave.";

  if (!props.hasLasers) {
    return (
      <section class="area engrave-area" aria-label="Engrave">
        <div class="empty">
          <span class="empty-mark" aria-hidden="true" />
          <h2>No lasers yet</h2>
          <p>Engraving needs a laser and one of its presets. Register the machine first.</p>
          <button type="button" class="btn" onClick={props.onOpenLasers}>
            Open Lasers
          </button>
        </div>
      </section>
    );
  }

  return (
    <section class="area engrave-area" aria-label="Engrave">
      <Show
        when={props.presets.length > 0}
        fallback={
          <div class="empty">
            <span class="empty-mark" aria-hidden="true" />
            <h2>No presets on {props.laserName ?? "this laser"}</h2>
            <p>Create a preset for this laser with its calibration before engraving an image.</p>
            <button type="button" class="btn" onClick={props.onOpenPresets}>
              Open Presets
            </button>
          </div>
        }
      >
        <div class="engrave">
          <aside class="engrave-side">
            <Field label="Preset for this tab">
              <select
                class="input"
                disabled={props.busy}
                onChange={(event) => {
                  const id = event.currentTarget.value;
                  event.currentTarget.value = tab()?.presetId ?? "";
                  if (id !== tab()?.presetId) props.onPreset(id);
                }}
              >
                <option value="" selected={!tab()?.presetId}>
                  Choose a preset
                </option>
                <For each={props.presets}>
                  {(item) => (
                    <option value={item.id} selected={item.id === tab()?.presetId}>
                      {item.name}
                      {item.has_scan ? "" : " (no scan)"}
                    </option>
                  )}
                </For>
              </select>
            </Field>

            <div class="engrave-file">
              <span class="file mono" title={tab()?.photoPath ?? undefined}>
                {tab()?.photoPath ? baseName(tab()!.photoPath!) : "No image chosen"}
              </span>
            </div>

            <div class="row-actions">
              <button type="button" class="btn" disabled={!canProcess()} onClick={props.onChoose}>
                {props.working === "process"
                  ? "Processing…"
                  : tab()?.photoPath
                    ? "Choose another…"
                    : "Choose image…"}
              </button>
              <Show when={tab()?.photoPath && !props.result}>
                <button type="button" class="btn" disabled={!canProcess()} onClick={props.onReprocess}>
                  Process again
                </button>
              </Show>
            </div>
            <p class="hint">PNG, JPG, WebP, BMP, or TIFF. Convert HEIC photos first.</p>

            <Show when={props.preset && !props.presetSelected}>
              <div class="scope-warning">
                <p class="hint warn">
                  This window is currently using “{props.preset?.name}” for processing.
                </p>
                <button type="button" class="btn small" disabled={props.busy} onClick={props.onUsePreset}>
                  Use this tab’s preset
                </button>
              </div>
            </Show>

            <div class="hint-block">
              <p class="hint">
                Engrave the gauge exported from this preset, then import a cropped, aligned scan at exactly{" "}
                <strong class="mono">
                  {gauge() ? gauge()!.block_size * gauge()!.columns : 0} ×{" "}
                  {gauge() ? gauge()!.block_size * gauge()!.rows : 0} px
                </strong>
                . The scan is never warped to fit.
              </p>
              <p class="hint">
                Saving a preset with new geometry or processing settings invalidates this tab’s result.
                Process the image again after saving.
              </p>
            </div>
          </aside>

          <div class="engrave-main">
            <Show when={props.staleReason}>
              {(reason) => <p class="hint warn engrave-warning">{reason()}</p>}
            </Show>

            <div class="previews">
              <Preview
                title="Engraving"
                tag={props.result ? `${props.result.width} × ${props.result.height}` : "—"}
                src={props.result?.preview_data_url}
                empty={emptyText()}
              />
              <Preview
                title="Simulation"
                tag={props.preset ? props.preset.name : "—"}
                src={props.result?.simulation_data_url}
                empty={emptyText()}
              />
            </div>

            <div class="engrave-foot">
              <div class="row-actions">
                <button
                  type="button"
                  class="btn"
                  disabled={!canSave()}
                  onClick={() => props.onSave("engraving")}
                >
                  {props.working === "save-engraving" ? "Saving…" : "Save engraving PNG…"}
                </button>
                <button
                  type="button"
                  class="btn"
                  disabled={!canSave()}
                  onClick={() => props.onSave("simulation")}
                >
                  {props.working === "save-simulation" ? "Saving…" : "Save simulation PNG…"}
                </button>
                <button
                  type="button"
                  class="btn"
                  disabled={!canSave()}
                  onClick={() => props.onSave("report")}
                >
                  {props.working === "save-report" ? "Saving…" : "Save report CSV…"}
                </button>
              </div>
              <span class="hint">
                {props.result ? "Saves use this tab’s processed image." : "Saves unlock after processing."}
              </span>
            </div>
          </div>
        </div>
      </Show>
    </section>
  );
}
