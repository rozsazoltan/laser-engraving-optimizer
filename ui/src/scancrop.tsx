import { For, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import type { ScanGeometry, ScanSource } from "./types";

type Crop = { x: number; y: number; width: number; height: number };

export function SampleOverlay(props: { geometry: ScanGeometry; class?: string }) {
  return (
    <svg class={`sample-overlay ${props.class ?? ""}`} viewBox={`0 0 ${props.geometry.width} ${props.geometry.height}`} preserveAspectRatio="none" aria-label="Sampled pixels" role="img">
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
  geometry: ScanGeometry | null;
  targetWidth: number;
  targetHeight: number;
  busy: boolean;
  canImport: boolean;
  editingSaved: boolean;
  sourceMode: "original" | "current" | null;
  onSourceMode: (mode: "original" | "current") => void;
  onImport: (crop: Crop, angle: number) => void;
  onCancel: () => void;
};

const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, Math.round(value)));

function rotatedSize(width: number, height: number, angle: number) {
  const radians = angle * Math.PI / 180;
  let cosine = Math.abs(Math.cos(radians));
  let sine = Math.abs(Math.sin(radians));
  if (cosine < 1e-12) cosine = 0;
  if (sine < 1e-12) sine = 0;
  return {
    width: Math.max(1, Math.ceil(width * cosine + height * sine)),
    height: Math.max(1, Math.ceil(width * sine + height * cosine)),
  };
}

function initialCrop(source: { width: number; height: number }, ratio: number): Crop {
  const width = Math.min(source.width, Math.round(source.height * ratio));
  const height = Math.min(source.height, Math.round(source.width / ratio));
  const fittedWidth = Math.min(width, Math.round(height * ratio));
  const fittedHeight = Math.min(height, Math.round(fittedWidth / ratio));
  return {
    x: Math.floor((source.width - fittedWidth) / 2),
    y: Math.floor((source.height - fittedHeight) / 2),
    width: fittedWidth,
    height: fittedHeight,
  };
}

function normalizedAngle(value: number) {
  return Math.max(-180, Math.min(180, Math.round(value * 10) / 10));
}

function angleDelta(value: number) {
  return ((value + 180) % 360 + 360) % 360 - 180;
}

export function ScanCropEditor(props: Props) {
  const ratio = () => props.targetWidth / props.targetHeight;
  const [angle, setAngle] = createSignal(0);
  const [angleInput, setAngleInput] = createSignal("0");
  const canvasSize = () => rotatedSize(props.source.width, props.source.height, angle());
  const [crop, setCrop] = createSignal(initialCrop(canvasSize(), ratio()));
  const [zoom, setZoom] = createSignal(1);
  const [drag, setDrag] = createSignal<
    | { mode: "move" | "resize"; px: number; py: number; crop: Crop }
    | { mode: "rotate"; pointerAngle: number; startAngle: number }
    | null
  >(null);
  let canvasElement: HTMLDivElement | undefined;

  createEffect(() => {
    const source = props.source;
    props.sourceMode;
    setAngle(0);
    setAngleInput("0");
    setCrop(initialCrop(source, ratio()));
    setZoom(1);
    setDrag(null);
  });

  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape" && !props.busy) props.onCancel();
  };
  onMount(() => window.addEventListener("keydown", closeOnEscape));
  onCleanup(() => window.removeEventListener("keydown", closeOnEscape));

  const baseCrop = () => initialCrop(props.source, ratio());
  const fitCrop = () => initialCrop(canvasSize(), ratio());
  const baseScale = () => Math.min(740 / props.source.width, 380 / props.source.height) * zoom();
  const scale = () => Math.min(
    baseCrop().width * baseScale() / fitCrop().width,
    baseCrop().height * baseScale() / fitCrop().height,
  );
  const displayWidth = () => canvasSize().width * scale();
  const displayHeight = () => canvasSize().height * scale();
  const sourceMin = () => Math.min(32, canvasSize().width, canvasSize().height);
  const maxCropWidth = (x: number, y: number) =>
    Math.floor(Math.min(canvasSize().width - x, (canvasSize().height - y) * ratio()));

  const setPosition = (axis: "x" | "y", value: number) =>
    setCrop((current) => {
      const next = { ...current, [axis]: Math.round(value) };
      next.x = clamp(next.x, 0, canvasSize().width - next.width);
      next.y = clamp(next.y, 0, canvasSize().height - next.height);
      return next;
    });

  const setDimension = (axis: "width" | "height", value: number) =>
    setCrop((current) => {
      const requestedWidth = axis === "width" ? value : value * ratio();
      const width = clamp(requestedWidth, sourceMin(), maxCropWidth(current.x, current.y));
      const height = Math.max(sourceMin(), Math.round(width / ratio()));
      return {
        ...current,
        width: Math.min(width, Math.floor((canvasSize().height - current.y) * ratio())),
        height: Math.min(height, canvasSize().height - current.y),
      };
    });

  const setAngleValue = (value: number, resetCrop: boolean) => {
    const next = normalizedAngle(value);
    setAngle(next);
    setAngleInput(String(next));
    if (resetCrop) setCrop(initialCrop(rotatedSize(props.source.width, props.source.height, next), ratio()));
  };

  const beginCropDrag = (event: PointerEvent, mode: "move" | "resize") => {
    if (props.busy) return;
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    setDrag({ mode, px: event.clientX, py: event.clientY, crop: { ...crop() } });
  };

  const beginRotate = (event: PointerEvent) => {
    if (props.busy || !canvasElement) return;
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    const rect = canvasElement.getBoundingClientRect();
    setDrag({
      mode: "rotate",
      pointerAngle: Math.atan2(event.clientY - (rect.top + rect.height / 2), event.clientX - (rect.left + rect.width / 2)) * 180 / Math.PI,
      startAngle: angle(),
    });
  };

  const moveDrag = (event: PointerEvent) => {
    const active = drag();
    if (!active) return;
    if (active.mode === "rotate") {
      if (!canvasElement) return;
      const rect = canvasElement.getBoundingClientRect();
      const pointerAngle = Math.atan2(event.clientY - (rect.top + rect.height / 2), event.clientX - (rect.left + rect.width / 2)) * 180 / Math.PI;
      const raw = active.startAngle + angleDelta(pointerAngle - active.pointerAngle);
      const next = normalizedAngle(event.shiftKey ? Math.round(raw / 15) * 15 : raw);
      setAngle(next);
      setAngleInput(String(next));
      setCrop(initialCrop(rotatedSize(props.source.width, props.source.height, next), ratio()));
      return;
    }
    const dx = (event.clientX - active.px) / scale();
    const dy = (event.clientY - active.py) / scale();
    if (active.mode === "move") {
      setCrop({
        ...active.crop,
        x: clamp(active.crop.x + dx, 0, canvasSize().width - active.crop.width),
        y: clamp(active.crop.y + dy, 0, canvasSize().height - active.crop.height),
      });
      return;
    }
    const widthDelta = Math.abs(dx) > Math.abs(dy * ratio()) ? dx : dy * ratio();
    const width = clamp(active.crop.width + widthDelta, sourceMin(), maxCropWidth(active.crop.x, active.crop.y));
    const height = Math.min(Math.round(width / ratio()), canvasSize().height - active.crop.y);
    setCrop({ ...active.crop, width: Math.round(height * ratio()), height });
  };

  const endDrag = () => {
    if (drag()?.mode === "rotate") setCrop(initialCrop(canvasSize(), ratio()));
    setDrag(null);
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

  const boxStyle = () => {
    const fitted = fitCrop();
    if (crop().x === fitted.x && crop().y === fitted.y && crop().width === fitted.width && crop().height === fitted.height) {
      const width = baseCrop().width * baseScale();
      const height = baseCrop().height * baseScale();
      return {
        left: `${(canvasSize().width * scale() - width) / 2}px`,
        top: `${(canvasSize().height * scale() - height) / 2}px`,
        width: `${width}px`,
        height: `${height}px`,
      };
    }
    return {
      left: `${crop().x * scale()}px`,
      top: `${crop().y * scale()}px`,
      width: `${crop().width * scale()}px`,
      height: `${crop().height * scale()}px`,
    };
  };
  const valid = () =>
    crop().width >= 32 && crop().height >= 32 &&
    crop().x >= 0 && crop().y >= 0 &&
    crop().x + crop().width <= canvasSize().width &&
    crop().y + crop().height <= canvasSize().height;

  return (
    <div class="scan-editor-scrim">
      <section class="scan-crop-editor" role="dialog" aria-modal="true" aria-label="Edit calibration scan">
        <header class="scan-editor-head">
          <div><span class="section-label">Calibration scan</span><h2>Edit crop</h2></div>
          {props.editingSaved && <div class="source-switch" role="group" aria-label="Scan source">
            <button type="button" class={props.sourceMode === "original" ? "active" : ""} aria-pressed={props.sourceMode === "original"} disabled={props.busy} onClick={() => props.onSourceMode("original")}>Original</button>
            <button type="button" class={props.sourceMode === "current" ? "active" : ""} aria-pressed={props.sourceMode === "current"} disabled={props.busy} onClick={() => props.onSourceMode("current")}>Current scan</button>
          </div>}
          <button type="button" class="btn" disabled={props.busy} onClick={props.onCancel}>Cancel</button>
        </header>
        <div class="crop-toolbar">
          <span class="section-label">Position crop · arrow keys move by 1 px, Shift + arrow by 10 px</span>
          <div class="row-actions">
            <button type="button" class="btn small" disabled={props.busy} onClick={() => setZoom((value) => Math.max(0.5, Math.round((value - 0.25) * 100) / 100))}>−</button>
            <span class="mono crop-zoom">{Math.round(zoom() * 100)}%</span>
            <button type="button" class="btn small" disabled={props.busy} onClick={() => setZoom((value) => Math.min(3, Math.round((value + 0.25) * 100) / 100))}>+</button>
          </div>
        </div>
        <div class="crop-viewport" tabindex="0" onKeyDown={nudge} aria-label="Scan crop preview. Use arrow keys to move one source pixel.">
          <div ref={canvasElement} class="crop-canvas" style={{
            width: `${displayWidth()}px`,
            height: `${displayHeight()}px`,
            transform: `translateY(${(props.source.height * baseScale() - displayHeight()) / 2}px)`,
          }}>
            <div class="rotated-image" style={{
              width: `${props.source.width * scale()}px`,
              height: `${props.source.height * scale()}px`,
              transform: `translate(-50%, -50%) rotate(${angle()}deg)`,
            }}>
              <img src={props.source.data_url} alt="Selected calibration scan" draggable={false} />
            </div>
            <div
              class="crop-window"
              style={boxStyle()}
              onPointerDown={(event) => beginCropDrag(event, "move")}
              onPointerMove={moveDrag}
              onPointerUp={endDrag}
              onPointerCancel={endDrag}
            >
              {props.geometry && <SampleOverlay geometry={props.geometry} />}
              <button type="button" class="crop-handle" aria-label="Resize crop" disabled={props.busy}
                onPointerDown={(event) => beginCropDrag(event, "resize")} onPointerMove={moveDrag}
                onPointerUp={endDrag} onPointerCancel={endDrag} />
              <button type="button" class="rotation-handle" aria-label="Rotate image" disabled={props.busy}
                onPointerDown={beginRotate} onPointerMove={moveDrag} onPointerUp={endDrag} onPointerCancel={endDrag}>
                ↻
              </button>
            </div>
          </div>
        </div>
        <div class="crop-fields-head">
          <span class="section-label">Rotated canvas {canvasSize().width} × {canvasSize().height} px</span>
        </div>
        <div class="crop-fields">
          <label><span class="field-label">X (px)</span><input class="input mono" type="number" min="0" max={canvasSize().width} step="1" disabled={props.busy} value={crop().x} onInput={(event) => setPosition("x", Number(event.currentTarget.value))} /></label>
          <label><span class="field-label">Y (px)</span><input class="input mono" type="number" min="0" max={canvasSize().height} step="1" disabled={props.busy} value={crop().y} onInput={(event) => setPosition("y", Number(event.currentTarget.value))} /></label>
          <label><span class="field-label">W (px)</span><input class="input mono" type="number" min="32" max={canvasSize().width} step="1" disabled={props.busy} value={crop().width} onInput={(event) => setDimension("width", Number(event.currentTarget.value))} /></label>
          <label><span class="field-label">H (px)</span><input class="input mono" type="number" min="32" max={canvasSize().height} step="1" disabled={props.busy} value={crop().height} onInput={(event) => setDimension("height", Number(event.currentTarget.value))} /></label>
          <label><span class="field-label">Rotation (deg)</span><input class="input mono" type="number" min="-180" max="180" step="0.1" disabled={props.busy} value={angleInput()} onInput={(event) => {
            const text = event.currentTarget.value;
            setAngleInput(text);
            if (text.trim() === "") return;
            const value = Number(text);
            if (Number.isFinite(value)) setAngleValue(value, true);
          }} onBlur={() => {
            const value = Number(angleInput());
            if (angleInput().trim() === "" || !Number.isFinite(value)) setAngleInput(String(angle()));
            else setAngleValue(value, true);
          }} /></label>
        </div>
        <p class="hint mono crop-size">{crop().width} × {crop().height} → {props.targetWidth} × {props.targetHeight} px</p>
        {!props.canImport && <p class="hint warn">This saved scan can be previewed, but saving its crop needs host support.</p>}
        <div class="row-actions">
          <button type="button" class="btn primary" disabled={props.busy || !valid() || !props.canImport}
            onClick={() => props.onImport(crop(), angle())}>{props.busy ? (props.editingSaved ? "Saving…" : "Importing…") : (props.editingSaved ? "Save" : "Import")}</button>
          <button type="button" class="btn" disabled={props.busy} onClick={props.onCancel}>Cancel</button>
        </div>
      </section>
    </div>
  );
}

export type { Crop };
