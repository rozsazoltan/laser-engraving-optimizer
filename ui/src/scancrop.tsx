import { For, createSignal } from "solid-js";
import type { ScanGeometry, ScanRect, ScanSource } from "./types";

type Crop = { x: number; y: number; width: number; height: number };

export function SampleOverlay(props: { geometry: ScanGeometry; class?: string }) {
  return (
    <svg
      class={`sample-overlay ${props.class ?? ""}`}
      viewBox={`0 0 ${props.geometry.width} ${props.geometry.height}`}
      preserveAspectRatio="none"
      aria-label="Sampled pixels"
      role="img"
    >
      <For each={props.geometry.patches}>
        {(patch) => (
          <>
            <rect class="sample-strip" x={patch.left.x} y={patch.left.y} width={patch.left.width} height={patch.left.height} />
            <rect class="sample-strip" x={patch.right.x} y={patch.right.y} width={patch.right.width} height={patch.right.height} />
            <rect class="sample-engraved" x={patch.engraved.x} y={patch.engraved.y} width={patch.engraved.width} height={patch.engraved.height} />
          </>
        )}
      </For>
    </svg>
  );
}

type Props = {
  source: ScanSource;
  geometry: ScanGeometry;
  targetWidth: number;
  targetHeight: number;
  busy: boolean;
  onImport: (crop: Crop) => void;
  onCancel: () => void;
};

const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, Math.round(value)));

function initialCrop(source: ScanSource, ratio: number): Crop {
  let width = source.width;
  let height = Math.round(width / ratio);
  if (height > source.height) {
    height = source.height;
    width = Math.round(height * ratio);
  }
  return {
    x: Math.floor((source.width - width) / 2),
    y: Math.floor((source.height - height) / 2),
    width,
    height,
  };
}

export function ScanCropEditor(props: Props) {
  const ratio = () => props.targetWidth / props.targetHeight;
  const [crop, setCrop] = createSignal(initialCrop(props.source, ratio()));
  const [zoom, setZoom] = createSignal(1);
  const [drag, setDrag] = createSignal<{ mode: "move" | "resize"; px: number; py: number; crop: Crop } | null>(null);
  const scale = () => Math.min(740 / props.source.width, 380 / props.source.height) * zoom();
  const displayWidth = () => props.source.width * scale();
  const displayHeight = () => props.source.height * scale();
  const sourceMin = () => Math.min(32, props.source.width, props.source.height);
  const maxCropWidth = (x: number, y: number) =>
    Math.floor(Math.min(props.source.width - x, (props.source.height - y) * ratio()));

  const setPosition = (axis: "x" | "y", value: number) =>
    setCrop((current) => {
      const next = { ...current, [axis]: Math.round(value) };
      next.x = clamp(next.x, 0, props.source.width - next.width);
      next.y = clamp(next.y, 0, props.source.height - next.height);
      return next;
    });

  const setDimension = (axis: "width" | "height", value: number) =>
    setCrop((current) => {
      const requestedWidth = axis === "width" ? value : value * ratio();
      const width = clamp(requestedWidth, sourceMin(), maxCropWidth(current.x, current.y));
      const height = Math.max(sourceMin(), Math.round(width / ratio()));
      return {
        ...current,
        width: Math.min(width, Math.floor((props.source.height - current.y) * ratio())),
        height: Math.min(height, props.source.height - current.y),
      };
    });

  const beginDrag = (event: PointerEvent, mode: "move" | "resize") => {
    if (props.busy) return;
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    setDrag({ mode, px: event.clientX, py: event.clientY, crop: { ...crop() } });
  };

  const moveDrag = (event: PointerEvent) => {
    const active = drag();
    if (!active) return;
    const dx = (event.clientX - active.px) / scale();
    const dy = (event.clientY - active.py) / scale();
    if (active.mode === "move") {
      setCrop({
        ...active.crop,
        x: clamp(active.crop.x + dx, 0, props.source.width - active.crop.width),
        y: clamp(active.crop.y + dy, 0, props.source.height - active.crop.height),
      });
      return;
    }
    const widthDelta = Math.abs(dx) > Math.abs(dy * ratio()) ? dx : dy * ratio();
    const width = clamp(active.crop.width + widthDelta, sourceMin(), maxCropWidth(active.crop.x, active.crop.y));
    const height = Math.min(Math.round(width / ratio()), props.source.height - active.crop.y);
    setCrop({ ...active.crop, width: Math.round(height * ratio()), height });
  };

  const nudge = (event: KeyboardEvent) => {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
    if ((event.target as HTMLElement).closest("input")) return;
    event.preventDefault();
    const distance = event.shiftKey ? 10 : 1;
    const { x, y } = crop();
    if (event.key === "ArrowLeft") setPosition("x", x - distance);
    if (event.key === "ArrowRight") setPosition("x", x + distance);
    if (event.key === "ArrowUp") setPosition("y", y - distance);
    if (event.key === "ArrowDown") setPosition("y", y + distance);
  };

  const boxStyle = () => ({
    left: `${crop().x * scale()}px`,
    top: `${crop().y * scale()}px`,
    width: `${crop().width * scale()}px`,
    height: `${crop().height * scale()}px`,
  });

  const valid = () =>
    crop().width >= 32 &&
    crop().height >= 32 &&
    crop().x >= 0 &&
    crop().y >= 0 &&
    crop().x + crop().width <= props.source.width &&
    crop().y + crop().height <= props.source.height;

  return (
    <section class="scan-crop-editor" aria-label="Crop calibration scan">
      <div class="crop-toolbar">
        <span class="section-label">Position crop · arrow keys move by 1 px, Shift + arrow by 10 px</span>
        <div class="row-actions">
          <button type="button" class="btn small" disabled={props.busy} onClick={() => setZoom((value) => Math.max(0.5, Math.round((value - 0.25) * 100) / 100))}>−</button>
          <span class="mono crop-zoom">{Math.round(zoom() * 100)}%</span>
          <button type="button" class="btn small" disabled={props.busy} onClick={() => setZoom((value) => Math.min(3, Math.round((value + 0.25) * 100) / 100))}>+</button>
        </div>
      </div>
      <div class="crop-viewport" tabindex="0" onKeyDown={nudge} aria-label="Scan crop preview. Use arrow keys to move one source pixel.">
        <div class="crop-canvas" style={{ width: `${displayWidth()}px`, height: `${displayHeight()}px` }}>
          <img src={props.source.data_url} alt="Selected calibration scan" draggable={false} />
          <div
            class="crop-window"
            style={boxStyle()}
            onPointerDown={(event) => beginDrag(event, "move")}
            onPointerMove={moveDrag}
            onPointerUp={() => setDrag(null)}
            onPointerCancel={() => setDrag(null)}
          >
            <SampleOverlay geometry={props.geometry} />
            <button
              type="button"
              class="crop-handle"
              aria-label="Resize crop"
              disabled={props.busy}
              onPointerDown={(event) => beginDrag(event, "resize")}
              onPointerMove={moveDrag}
              onPointerUp={() => setDrag(null)}
              onPointerCancel={() => setDrag(null)}
            />
          </div>
        </div>
      </div>
      <div class="crop-fields">
        <label><span class="field-label">X (px)</span><input class="input mono" type="number" min="0" max={props.source.width} step="1" disabled={props.busy} value={crop().x} onInput={(event) => setPosition("x", Number(event.currentTarget.value))} /></label>
        <label><span class="field-label">Y (px)</span><input class="input mono" type="number" min="0" max={props.source.height} step="1" disabled={props.busy} value={crop().y} onInput={(event) => setPosition("y", Number(event.currentTarget.value))} /></label>
        <label><span class="field-label">W (px)</span><input class="input mono" type="number" min="32" max={props.source.width} step="1" disabled={props.busy} value={crop().width} onInput={(event) => setDimension("width", Number(event.currentTarget.value))} /></label>
        <label><span class="field-label">H (px)</span><input class="input mono" type="number" min="32" max={props.source.height} step="1" disabled={props.busy} value={crop().height} onInput={(event) => setDimension("height", Number(event.currentTarget.value))} /></label>
      </div>
      <p class="hint mono crop-size">{crop().width} × {crop().height} → {props.targetWidth} × {props.targetHeight} px</p>
      <div class="row-actions">
        <button type="button" class="btn primary" disabled={props.busy || !valid()} onClick={() => props.onImport(crop())}>{props.busy ? "Importing…" : "Import"}</button>
        <button type="button" class="btn" disabled={props.busy} onClick={props.onCancel}>Cancel</button>
      </div>
    </section>
  );
}

export type { Crop };
