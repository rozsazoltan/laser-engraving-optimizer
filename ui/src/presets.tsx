import { For, Show, createEffect, createSignal } from "solid-js";
import { COLOR_METHODS, formatPower, laserLabel, powerWatts, type CalibrationSpec, type ColorMethod, type Draft, type Laser, type Preset, type PresetPreview, type ProcessingSettings, type ScanGeometry, type ScanSource, type SpeedUnit } from "./types";
import { Field, NumberField } from "./fields";
import { SampleOverlay, ScanCropEditor, type Crop } from "./scancrop";

type Props = {
  lasers: Laser[];
  selectedLaser: string | null;
  presets: Preset[];
  preset: Preset | null;
  draft: Draft | null;
  dirty: boolean;
  busy: boolean;
  working: string | null;
  operationError: string | null;
  preview: PresetPreview | null;
  previewLoading: boolean;
  previewError: string | null;
  geometry: ScanGeometry | null;
  cropSource: ScanSource | null;
  cropCanImport: boolean;
  cropEditingSaved: boolean;
  cropSourceMode: "original" | "current" | null;
  canEditScan: boolean;
  speedUnit: SpeedUnit;
  onScope: (laserId: string) => void;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onEdit: (patch: Partial<Draft>) => void;
  onGeometry: (patch: Partial<CalibrationSpec>) => void;
  onProcessing: (patch: Partial<ProcessingSettings>) => void;
  onSave: () => void;
  onRevert: () => void;
  onRemove: () => void;
  onImportScan: () => void;
  onEditScan: () => void;
  onCropImport: (crop: Crop, angle: number) => void;
  onCropSource: (mode: "original" | "current") => void;
  onCropCancel: () => void;
  onRemoveScan: () => void;
  onExportGauge: () => void;
  onOpenArchive: () => void;
  onClearArchive: () => void;
  onOpenLasers: () => void;
};

function TrashIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M2.6 4.4h10.8" />
      <path d="M6.2 4.4V2.9h3.6v1.5" />
      <path d="M4.1 4.4l.7 8.1a1.1 1.1 0 0 0 1.1 1h4.2a1.1 1.1 0 0 0 1.1-1l.7-8.1" />
      <path d="M6.7 7v4M9.3 7v4" />
    </svg>
  );
}

function PreviewImage(props: { src: string; alt: string }) {
  const [failed, setFailed] = createSignal(false);
  createEffect(() => {
    props.src;
    setFailed(false);
  });
  return <Show when={!failed()} fallback={<div class="preview-error" role="status">Could not display preview</div>}>
    <img src={props.src} alt={props.alt} onError={() => setFailed(true)} />
  </Show>;
}

export function PresetsArea(props: Props) {
  const [confirmClear, setConfirmClear] = createSignal(false);
  const [showSampleInfo, setShowSampleInfo] = createSignal(false);
  const laser = () => props.lasers.find((item) => item.id === props.selectedLaser) ?? null;
  const gaugeWidth = () => (props.draft ? props.draft.calibration.block_size * props.draft.calibration.columns : 0);
  const gaugeHeight = () => (props.draft ? props.draft.calibration.block_size * props.draft.calibration.rows : 0);

  return (
    <section class="area presets-area" aria-label="Presets">
      <Show
        when={props.lasers.length > 0}
        fallback={
          <div class="empty">
            <span class="empty-mark" aria-hidden="true" />
            <h2>No lasers yet</h2>
            <p>A preset needs a laser first. Register the machine, then create presets for it.</p>
            <button type="button" class="btn" onClick={props.onOpenLasers}>
              Open Lasers
            </button>
          </div>
        }
      >
        <div class="area-head">
        <div>
          <h1>Presets</h1>
          <p class="area-note">
            Calibration and processing for one material on{" "}
            <strong>{laser() ? laserLabel(laser()!) : "the selected laser"}</strong>.
          </p>
        </div>
        <div class="area-controls">
          <label class="scope">
            <span class="field-label">Laser scope</span>
            <select
              class="input"
              aria-label="Laser scope for presets"
              disabled={props.busy}
              onChange={(event) => {
                const id = event.currentTarget.value;
                event.currentTarget.value = props.selectedLaser ?? "";
                if (id !== props.selectedLaser) props.onScope(id);
              }}
            >
              <For each={props.lasers}>
                {(item) => (
                  <option value={item.id} selected={item.id === props.selectedLaser}>
                    {laserLabel(item)}
                  </option>
                )}
              </For>
            </select>
          </label>
          <button type="button" class="btn" disabled={props.busy || !props.selectedLaser} onClick={props.onAdd}>
            {props.working === "preset-add" ? "Creating…" : "New preset"}
          </button>
        </div>
      </div>

      <div class="preset-split">
        <Show
          when={props.presets.length > 0}
          fallback={
            <div class="preset-side empty-side">
              <p class="hint">No presets on this laser yet.</p>
              <button type="button" class="btn small" disabled={props.busy} onClick={props.onAdd}>
                {props.working === "preset-add" ? "Creating…" : "Create the first preset"}
              </button>
            </div>
          }
        >
          <ul class="preset-list" aria-label="Presets on this laser">
            <For each={props.presets}>
              {(item) => (
                <li>
                  <button
                    type="button"
                    class={`preset-row ${item.id === props.preset?.id ? "selected" : ""}`}
                    disabled={props.busy}
                    onClick={() => props.onSelect(item.id)}
                    aria-pressed={item.id === props.preset?.id}
                  >
                    <span class="preset-name">{item.name}</span>
                    <span class="preset-meta">
                      <Show when={!item.has_scan}>
                        <span class="preset-warn" title="No calibration scan" aria-label="Incomplete preset">
                          !
                        </span>
                      </Show>
                      {item.has_scan ? "scan ready" : "no scan"}
                      <Show when={props.dirty && item.id === props.preset?.id}>
                        <span class="preset-dirty" aria-label="unsaved changes" />
                      </Show>
                    </span>
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>

        <Show when={props.draft} fallback={<div class="preset-editor" />}>
          {(edited) => {
            const current = () => props.preset!;
            return (
              <div class="preset-editor">
                <div class="editor-head">
                  <span class="section-label">
                    Editing preset
                    <Show when={props.dirty}>
                      <b class="dirty-flag">unsaved</b>
                    </Show>
                  </span>
                  <div class="row-actions">
                    <button type="button" class="btn primary" disabled={!props.dirty || props.busy} onClick={props.onSave}>
                      {props.working === "save" ? "Saving…" : "Save"}
                    </button>
                    <button type="button" class="btn" disabled={!props.dirty || props.busy} onClick={props.onRevert}>
                      Revert
                    </button>
                    <button type="button" class="btn danger" disabled={props.busy} onClick={props.onRemove}>
                      {props.working === "preset-remove" ? "Removing…" : "Remove"}
                    </button>
                  </div>
                </div>

                <div class="editor-section">
                  <div class="section-head"><span class="section-label">Recording settings</span></div>
                  <div class="grid-2">
                    <NumberField label={`Speed (${props.speedUnit})`} value={props.speedUnit === "mm/min" ? Math.round(edited().speed * 60) : edited().speed} min={0} max={props.speedUnit === "mm/min" ? 600000 : 10000} step={1} disabled={props.busy} hint="Recorded for reference only, they do not change processing. Keep them at the values the gauge was engraved with: a calibration is valid only for that speed, power and DPI." onValue={(value) => {
                      const speed = props.speedUnit === "mm/min" ? Math.round((value / 60) * 10) / 10 : value;
                      props.onEdit({ speed: Math.max(0, speed) });
                    }} />
                    <NumberField label="Power (%)" value={edited().power_percent} min={0} max={100} step={1} disabled={props.busy} hint="Recorded for reference only, they do not change processing. Keep them at the values the gauge was engraved with: a calibration is valid only for that speed, power and DPI." onValue={(power_percent) => props.onEdit({ power_percent })} />
                    <NumberField label="DPI" value={edited().dpi} min={0} max={10000} step={1} disabled={props.busy} hint="Recorded for reference only, they do not change processing. Keep them at the values the gauge was engraved with: a calibration is valid only for that speed, power and DPI." onValue={(dpi) => props.onEdit({ dpi })} />
                    <p class="hint power-hint">{formatPower(edited().power_percent, powerWatts(edited(), laser()))}</p>
                  </div>
                </div>

                <div class="grid-2">
                  <Field label="Name" flag={props.dirty}>
                    <input
                      class="input"
                      type="text"
                      autocomplete="off"
                      spellcheck={false}
                      disabled={props.busy}
                      value={edited().name}
                      onInput={(event) => props.onEdit({ name: event.currentTarget.value })}
                    />
                  </Field>
                  <Field label="Note">
                    <input
                      class="input"
                      type="text"
                      autocomplete="off"
                      placeholder="Material, thickness, lens"
                      disabled={props.busy}
                      value={edited().note}
                      onInput={(event) => props.onEdit({ note: event.currentTarget.value })}
                    />
                  </Field>
                </div>

                <div class="editor-section">
                  <div class="section-head">
                    <span class="section-label">Calibration geometry</span>
                    <span class="section-note mono">
                      {gaugeWidth()} × {gaugeHeight()} px
                    </span>
                  </div>
                  <div class="grid-2">
                    <NumberField
                      label="Block size"
                      value={edited().calibration.block_size}
                      min={32}
                      max={4096}
                      step={1}
                      disabled={props.busy}
                      hint="Size of one gauge cell in pixels. The gauge is columns × block size wide and rows × block size tall, so changing it changes the scan size you must photograph."
                      onValue={(block_size) => props.onGeometry({ block_size })}
                    />
                    <NumberField
                      label="Exponent"
                      value={edited().calibration.exponent}
                      min={0.1}
                      max={8}
                      step={0.05}
                      disabled={props.busy}
                      hint="Only changes how the gauge spreads its grey levels, not the photo tone curve. 1.0 is linear; 1.8 (default) puts more patches in the light and mid ranges where the laser response still changes. If the light patches stay white, lower it to 1.2-1.4. Changing it needs a new gauge and a new scan."
                      onValue={(exponent) => props.onGeometry({ exponent })}
                    />
                    <NumberField
                      label="Rows"
                      value={edited().calibration.rows}
                      min={2}
                      max={64}
                      step={1}
                      disabled={props.busy}
                      hint="Number of gauge rows. The top-left cell stays unburned as a paper reference; the remaining cells step from light to dark."
                      onValue={(rows) => props.onGeometry({ rows })}
                    />
                    <NumberField
                      label="Columns"
                      value={edited().calibration.columns}
                      min={2}
                      max={64}
                      step={1}
                      disabled={props.busy}
                      hint="Number of gauge columns. Rows × columns − 1 cells are measured, so more cells give a finer calibration."
                      onValue={(columns) => props.onGeometry({ columns })}
                    />
                  </div>
                </div>

                <div class="editor-section">
                  <div class="section-head">
                    <span class="section-label">Processing</span>
                  </div>
                  <div class="grid-2">
                    <NumberField
                      label="Gamma"
                      value={edited().processing.gamma}
                      min={0.1}
                      max={4}
                      step={0.05}
                      disabled={props.busy}
                      hint="Tone curve applied to the photo. 1.0 is neutral, higher values lighten the midtones (gentler portraits), lower values darken them."
                      onValue={(gamma) => props.onProcessing({ gamma })}
                    />
                    <NumberField
                      label="CLAHE clip"
                      value={edited().processing.clahe_clip}
                      min={0}
                      max={16}
                      step={0.1}
                      disabled={props.busy}
                      hint="Local contrast boost. 0 turns it off, 4 is the default; higher values add contrast and grain, lower values keep the surface calm."
                      onValue={(clahe_clip) => props.onProcessing({ clahe_clip })}
                    />
                    <Field label="Colour method" span hint="Decolorize keeps tonal separation between colours (legacy behaviour). Luma is a plain weighted grey conversion.">
                      <select
                        class="input"
                        disabled={props.busy}
                        onChange={(event) =>
                          props.onProcessing({ color_method: event.currentTarget.value as ColorMethod })
                        }
                      >
                        <For each={COLOR_METHODS}>
                          {(method) => (
                            <option
                              value={method}
                              selected={method === edited().processing.color_method}
                            >
                              {method}
                            </option>
                          )}
                        </For>
                      </select>
                    </Field>
                  </div>
                </div>

                <div class="editor-section">
                  <div class="section-head">
                    <span class="section-label">Calibration scan</span>
                    <span class={`scan-state ${current().has_scan ? "ready" : ""}`}>
                      <span class={`dot ${current().has_scan ? "on" : ""}`} aria-hidden="true" />
                      {current().has_scan ? "Scan ready" : "No scan yet"}
                    </span>
                  </div>
                  <div class="row-actions">
                    <button type="button" class="btn" disabled={props.busy || !!props.cropSource} onClick={props.onImportScan}>
                      {props.working === "scan-picker" ? "Choose scan…" : "Import calibration scan…"}
                    </button>
                    <button type="button" class="btn" disabled={props.busy} onClick={props.onExportGauge}>
                      {props.working === "gauge-picker" ? "Choose location…" : props.working === "gauge" ? "Exporting…" : "Export gauge PNG…"}
                    </button>
                  </div>
                  <Show when={!props.cropSource && ["scan-picker", "scan-source", "scan-remove"].includes(props.working ?? "")}>
                    <p class="operation-note" role="status">
                      {props.working === "scan-picker" ? "Choose a calibration scan…" : props.working === "scan-source" ? "Loading calibration scan…" : "Moving calibration scan to archive…"}
                    </p>
                  </Show>
                  <p class="hint">
                    Engrave the exported gauge, then import a cropped, aligned photo of it at exactly{" "}
                    <strong class="mono">
                      {gaugeWidth()} × {gaugeHeight()} px
                    </strong>
                    . engraving never warps the scan to fit.
                  </p>
                  <Show when={props.dirty}>
                    <p class="hint warn">Scans and gauges use the saved calibration. Save to apply unsaved values.</p>
                  </Show>
                  <div class="scan-previews">
                    <div class="scan-preview-slot">
                      <div class="scan-slot-head">
                        <span class="field-label">Calibration scan</span>
                        <button
                          type="button"
                          class="btn small"
                          disabled={props.busy || !props.canEditScan}
                          onClick={props.onEditScan}
                        >
                          {props.working === "scan-source" ? "Opening…" : "Edit scan…"}
                        </button>
                        <button
                          type="button"
                          class="btn icon"
                          title="Move scan to the archive"
                          aria-label="Move scan to the archive"
                          disabled={props.busy || !current().has_scan}
                          onClick={props.onRemoveScan}
                        >
                          <TrashIcon />
                        </button>
                      </div>
                      <Show when={props.preview?.scan_data_url} fallback={<div class="scan-placeholder" role={props.previewLoading ? "status" : undefined} style={{ "aspect-ratio": `${gaugeWidth()} / ${gaugeHeight()}` }}>{props.previewLoading ? "Loading calibration scan…" : props.previewError ? "Calibration preview failed" : "No calibration scan yet"}</div>}>
                        {(src) => <div class="scan-preview-frame" style={{ width: `min(100%, ${Math.round(220 * gaugeWidth() / gaugeHeight())}px)` }}>
                          <PreviewImage src={src()} alt="Calibration scan preview" />
                          <Show when={props.geometry}>{(geometry) => <SampleOverlay geometry={geometry()} />}</Show>
                        </div>}
                      </Show>
                    </div>
                    <div class="scan-preview-slot">
                      <div class="scan-slot-head">
                        <span class="field-label">Sample gauge</span>
                      </div>
                      <Show when={props.preview} fallback={<div class="scan-placeholder" role="status" style={{ "aspect-ratio": `${gaugeWidth()} / ${gaugeHeight()}` }}>{props.previewLoading ? "Loading preview…" : props.previewError ? "Preview unavailable" : "Preview not ready"}</div>}>
                        {(preview) => <div class="scan-preview-frame" style={{ width: `min(100%, ${Math.round(220 * preview().width / preview().height)}px)` }}>
                          <PreviewImage src={preview().gauge_data_url} alt="Sample gauge preview" />
                          <Show when={props.geometry}>{(geometry) => <SampleOverlay geometry={geometry()} />}</Show>
                        </div>}
                      </Show>
                    </div>
                  </div>
                  <p class="hint mono preview-size">{props.preview ? `${props.preview.width} × ${props.preview.height} px` : `${gaugeWidth()} × ${gaugeHeight()} px`}{props.previewLoading ? " · Updating preview…" : ""}</p>
                  <Show when={props.previewError && !props.previewLoading}><p class="hint warn" role="status">Preview could not be updated: {props.previewError}</p></Show>
                  <div class="sample-legend">
                    <span class="sample-swatch" /> Sampled pixels
                    <button
                      type="button"
                      class="info-btn"
                      aria-expanded={showSampleInfo()}
                      aria-label="What the sampled pixels mean"
                      title="What the sampled pixels mean"
                      onClick={() => setShowSampleInfo((open) => !open)}
                    >
                      i
                    </button>
                  </div>
                  <Show when={showSampleInfo()}>
                    <p class="sample-info">
                      The boxes stay inside each patch on purpose: 15% of the patch width and 20% of
                      its height are skipped on every side, so laser edge bleed and wood grain cannot
                      skew the reading. The two thin strips just outside the patch measure the
                      unburned background. These margins are fixed; changing them would change the
                      fitted curve.
                    </p>
                  </Show>
                  <div class="row-actions">
                    <button type="button" class="btn" disabled={props.busy} onClick={props.onOpenArchive}>{props.working === "archive-open" ? "Opening archive…" : "Open archive"}</button>
                    <button type="button" class="btn icon" title="Empty the scan archive" aria-label="Empty the scan archive" disabled={props.busy} onClick={() => setConfirmClear(true)}>
                      <TrashIcon />
                    </button>
                  </div>
                  <Show when={confirmClear()}>
                    <div class="archive-confirm" role="alert">
                      <span>Empty archived scans?</span>
                      <button type="button" class="btn small" onClick={() => setConfirmClear(false)}>Keep</button>
                      <button type="button" class="btn small danger" disabled={props.busy} onClick={() => { setConfirmClear(false); props.onClearArchive(); }}>{props.working === "archive-clear" ? "Emptying…" : "Empty archive"}</button>
                    </div>
                  </Show>
                  <Show when={props.working === "archive-clear"}>
                    <p class="operation-note" role="status">Emptying scan archive…</p>
                  </Show>
                </div>
              </div>
            );
          }}
        </Show>
      </div>
      </Show>
      <Show when={props.cropSource && props.preset}>
        {(session) => <ScanCropEditor
          source={props.cropSource!}
          geometry={props.geometry}
          targetWidth={props.preset!.calibration.block_size * props.preset!.calibration.columns}
          targetHeight={props.preset!.calibration.block_size * props.preset!.calibration.rows}
          busy={props.busy}
          canImport={props.cropCanImport}
          editingSaved={props.cropEditingSaved}
          sourceMode={props.cropSourceMode}
          working={props.working}
          operationError={props.operationError}
          onSourceMode={props.onCropSource}
          onImport={props.onCropImport}
          onCancel={props.onCropCancel}
        />}
      </Show>
    </section>
  );
}
