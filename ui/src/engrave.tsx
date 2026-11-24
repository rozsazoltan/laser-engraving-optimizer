import { For, Show, createEffect, createSignal, onMount } from "solid-js";
import { Field } from "./fields";
import type { DocTab, JobEntry, Preset, ProcessResult } from "./types";

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
                <Show when={tab.photoPath || tab.entries.length}>
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
  const [imageState, setImageState] = createSignal<"loading" | "ready" | "error">("loading");
  let image: HTMLImageElement | undefined;
  const settleLoadedImage = (source?: string) => {
    if (!source || !image?.complete) return;
    const expected = new URL(source, document.baseURI).href;
    if (image.currentSrc === expected) setImageState(image.naturalWidth > 0 ? "ready" : "error");
  };
  createEffect(() => {
    const source = props.src;
    setImageState("loading");
    settleLoadedImage(source);
  });
  onMount(() => settleLoadedImage(props.src));
  return (
    <figure class="preview">
      <figcaption class="preview-head">
        <span>{props.title}</span>
        <span class="preview-tag mono">{props.tag}</span>
      </figcaption>
      <div class="preview-body">
        <Show when={props.src} fallback={<p class="preview-empty">{props.empty}</p>}>
          {(src) => <>
            <Show when={imageState() === "loading"}><p class="preview-empty" role="status">Loading {props.title.toLowerCase()} preview…</p></Show>
            <Show when={imageState() === "error"}><p class="preview-empty" role="alert">Could not display {props.title.toLowerCase()} preview</p></Show>
            <img ref={image} src={src()} alt={`${props.title} preview`} draggable={false}
              onLoad={() => setImageState("ready")} onError={() => setImageState("error")} />
          </>}
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
  historyIndex: number;
  historyCount: number;
  historyEntry: JobEntry | null;
  isLatest: boolean;
  staleReason: string | null;
  busy: boolean;
  working: string | null;
  hasLasers: boolean;
  onPreset: (id: string) => void;
  onUsePreset: () => void;
  onChoose: () => void;
  onReprocess: () => void;
  onPrevious: () => void;
  onNext: () => void;
  onSave: (kind: "engraving" | "simulation" | "report") => void;
  onOpenPresets: () => void;
  onOpenLasers: () => void;
};

export function EngraveArea(props: EngraveProps) {
  const tab = () => props.tab;
  const gauge = () => (props.preset ? props.preset.calibration : null);
  const canProcess = () =>
    !props.busy && !!tab()?.presetId && props.preset?.has_scan === true && props.presetSelected;

  const canSave = () => !!props.result && !props.busy && props.presetSelected && props.isLatest && !props.staleReason;
  const historical = () => props.historyCount > 0 && !props.isLatest;
  const canReprocess = () => canProcess() && !!tab()?.photoPath;
  const processing = () => props.working === "process" || props.working === "image-decode";
  const processingError = () => tab()?.processingError;

  const emptyText = () =>
    processing() ? "Preview pending…" : processingError() ? "No preview available." :
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
                {props.working === "image-picker"
                  ? "Choosing image…"
                  : props.working === "image-decode"
                    ? "Preparing image…"
                    : props.working === "process"
                  ? "Processing…"
                  : tab()?.photoPath
                    ? "Choose another…"
                    : "Choose image…"}
              </button>
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
            <div class="history-toolbar" aria-label="Result history">
              <div class="history-nav">
                <button type="button" class="icon-btn" aria-label="Previous result" title="Previous result (Alt+Left)"
                  disabled={props.busy || props.historyIndex <= 0} onClick={props.onPrevious}>
                  <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M7.5 2.5 4 6l3.5 3.5" /></svg>
                </button>
                <button type="button" class="icon-btn" aria-label="Next result" title="Next result (Alt+Right)"
                  disabled={props.busy || props.historyIndex >= props.historyCount - 1} onClick={props.onNext}>
                  <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m4.5 2.5 3.5 3.5-3.5 3.5" /></svg>
                </button>
                <span class="history-step mono">
                  {props.historyEntry
                    ? `Result ${props.historyIndex + 1} / ${props.historyCount} · preset ${props.historyEntry.presetName} · ${new Date(props.historyEntry.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false })}`
                    : "No results yet"}
                </span>
              </div>
              <button type="button" class="icon-btn re-engrave" aria-label="Re-engrave" title="Re-engrave"
                disabled={!canReprocess()} onClick={props.onReprocess}>
                <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M9.5 4A3.8 3.8 0 0 0 3 2.8L2 4M2 4V1.8M2 4h2.2M2.5 8A3.8 3.8 0 0 0 9 9.2L10 8m0 0v2.2M10 8H7.8" /></svg>
              </button>
            </div>
            <Show when={historical()}>
              <p class="hint warn history-note">Viewing an earlier result. Saves are disabled until you return to the latest result.</p>
            </Show>
            <Show when={props.staleReason}>
              {(reason) => <p class="hint warn engrave-warning">{reason()}</p>}
            </Show>

            <Show when={processing()}>
              <p class="operation-note" role="status">
                Processing photo…{props.result ? " Showing the previous result." : " Previews will appear here."}
              </p>
            </Show>
            <Show when={!processing() && processingError()}>
              <div class="operation-note">
                <span class="warn" role="alert">{processingError()}{props.result ? " Showing the previous result." : ""}</span>
                <Show when={tab()?.photoPath}>
                  {" "}<button type="button" class="btn" disabled={!canReprocess()} onClick={props.onReprocess}>Retry processing</button>
                </Show>
              </div>
            </Show>

            <div class={`previews ${historical() ? "previews-historical" : ""}`}>
              <Preview
                title="Engraving"
                tag={props.result ? `${props.result.width} × ${props.result.height}` : "—"}
                src={props.result?.preview_data_url}
                empty={emptyText()}
              />
              <Preview
                title="Simulation"
                tag={props.historyEntry?.presetName ?? "—"}
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
                 {props.working === "save-engraving-picker" ? "Choose engraving location…" : props.working === "save-engraving" ? "Saving…" : "Save engraving PNG…"}
                </button>
                <button
                  type="button"
                  class="btn"
                  disabled={!canSave()}
                  onClick={() => props.onSave("simulation")}
                >
                   {props.working === "save-simulation-picker" ? "Choose simulation location…" : props.working === "save-simulation" ? "Saving…" : "Save simulation PNG…"}
                </button>
                <button
                  type="button"
                  class="btn"
                  disabled={!canSave()}
                  onClick={() => props.onSave("report")}
                >
                   {props.working === "save-report-picker" ? "Choose report location…" : props.working === "save-report" ? "Saving…" : "Save report CSV…"}
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
