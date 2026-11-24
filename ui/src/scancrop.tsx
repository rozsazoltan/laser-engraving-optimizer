import { For, batch, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { ScanGeometry, ScanSource } from "./types";

type Crop = { x: number; y: number; width: number; height: number; corners?: [number, number][] };

export function SampleOverlay(props: { geometry: ScanGeometry; class?: string; angle?: number }) {
  const patches = createMemo(() => {
    props.angle;
    return props.geometry.patches;
  });
  return (
    <svg class={`sample-overlay ${props.class ?? ""}`} viewBox={`0 0 ${props.geometry.width} ${props.geometry.height}`} preserveAspectRatio="none" aria-label="Sampled pixels" role="img">
      <For each={patches()}>
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
    height: Math.max(1, Math.ceil(height * cosine + width * sine)),
  };
}

function frameSize(size: { width: number; height: number }, ratio: number) {
  const width = Math.max(1, Math.round(Math.min(size.width, size.height * ratio)));
  return { width, height: clamp(width / ratio, 1, size.height) };
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
  const canvasSize = createMemo(() => rotatedSize(props.source.width, props.source.height, angle()));
  const baseFrame = createMemo(() => frameSize(canvasSize(), ratio()));
  const [zoom, setZoom] = createSignal(1);
  const frame = createMemo(() => {
    const base = baseFrame();
    const width = Math.max(1, Math.min(base.width, Math.round(base.width / zoom())));
    return { width, height: clamp(width / ratio(), 1, base.height) };
  });
  const [pan, setPan] = createSignal({ x: 0, y: 0 });
  const [perspective, setPerspective] = createSignal(false);
  const [quad, setQuad] = createSignal<[number, number][] | null>(null);
  const quadPoints = () => quad() ?? [];
  const [drag, setDrag] = createSignal<
    | { mode: "pan"; pointerId: number; px: number; py: number; scale: number; pan: { x: number; y: number } }
    | { mode: "rotate"; pointerId: number; pointerAngle: number; startAngle: number }
    | { mode: "scale"; pointerId: number; startX: number; startZoom: number }
    | { mode: "grip"; pointerId: number; startDist: number; startZoom: number }
    | { mode: "quad"; pointerId: number; index: number; startX: number; startY: number; points: [number, number][] }
    | null
  >(null);
  let canvasElement: HTMLDivElement | undefined;
  let stageElement: HTMLDivElement | undefined;
  let pendingPan: { x: number; y: number } | null = null;
  let panFrame = 0;
  const clearPendingPan = () => {
    if (panFrame) cancelAnimationFrame(panFrame);
    panFrame = 0;
    pendingPan = null;
  };
  const [viewportHeight, setViewportHeight] = createSignal(0);

  onMount(() => {
    const update = () => setViewportHeight(stageElement?.getBoundingClientRect().height ?? 0);
    update();
    window.addEventListener("resize", update);
    onCleanup(() => window.removeEventListener("resize", update));
  });

  createEffect(() => {
    const source = props.source;
    props.sourceMode;
    const saved = source.crop ?? null;
    const savedAngle = saved && Number.isFinite(saved.angle) ? normalizedAngle(saved.angle) : 0;
    clearPendingPan();
    batch(() => {
      setAngle(savedAngle);
      setPerspective(false);
      setQuad(null);
      setZoom(1);
      const canvas = rotatedSize(source.width, source.height, savedAngle);
      const base = frameSize(canvas, ratio());
      if (saved && saved.width >= 1 && saved.height >= 1) {
        setZoom(Math.max(1, Math.min(20, base.width / Math.max(1, Math.round(saved.width)))));
        setPan({ x: Math.round(saved.x), y: Math.round(saved.y) });
        if (saved.corners && saved.corners.length === 4) {
          setQuad(saved.corners.map(([x, y]) => [x, y] as [number, number]));
          setPerspective(true);
        }
      } else {
        setPan({
          x: Math.max(0, Math.round((source.width - base.width) / 2)),
          y: Math.max(0, Math.round((source.height - base.height) / 2)),
        });
      }
      setDrag(null);
    });
  });

  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape" && !props.busy) props.onCancel();
  };
  onMount(() => window.addEventListener("keydown", closeOnEscape));
  onCleanup(() => window.removeEventListener("keydown", closeOnEscape));
  onCleanup(clearPendingPan);

  const frameScreenWidth = () => {
    const available = viewportHeight() ? viewportHeight() - 16 : 380;
    return Math.max(120, Math.min(740, available * ratio(), 380 * ratio()));
  };
  const scale = () => frameScreenWidth() / frame().width;
  const crop = (): Crop => {
    const points = quad();
    const box = { x: Math.round(pan().x), y: Math.round(pan().y), ...frame() };
    return points ? { ...box, corners: points } : box;
  };
  const togglePerspective = () => {
    flushPan();
    batch(() => {
      if (perspective()) {
        setPerspective(false);
        setQuad(null);
        return;
      }
      const box = crop();
      setQuad([
        [box.x, box.y],
        [box.x + box.width, box.y],
        [box.x + box.width, box.y + box.height],
        [box.x, box.y + box.height],
      ]);
      setPerspective(true);
    });
  };
  const outOfBounds = () => {
    const size = canvasSize();
    const box = frame();
    const position = pan();
    return (
      position.x < 0 ||
      position.y < 0 ||
      position.x + box.width > size.width ||
      position.y + box.height > size.height
    );
  };

  const setAngleValue = (value: number) => {
    if (!Number.isFinite(value) || perspective()) return;
    flushPan();
    const before = crop();
    const next = normalizedAngle(value);
    batch(() => {
      setAngle(next);
      const box = frame();
      setPan({
        x: before.x + (before.width - box.width) / 2,
        y: before.y + (before.height - box.height) / 2,
      });
    });
  };

  const setZoomValue = (value: number) => {
    if (!Number.isFinite(value)) return;
    flushPan();
    const before = crop();
    const next = Math.max(1, Math.min(20, Math.round(value * 1000) / 1000));
    batch(() => {
      setZoom(next);
      const box = frame();
      setPan({
        x: before.x + (before.width - box.width) / 2,
        y: before.y + (before.height - box.height) / 2,
      });
    });
  };

  const beginScale = (event: PointerEvent) => {
    event.stopPropagation();
    if (props.busy || drag() || event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLButtonElement;
    handle.focus({ preventScroll: true });
    handle.setPointerCapture(event.pointerId);
    setDrag({ mode: "scale", pointerId: event.pointerId, startX: event.clientX, startZoom: zoom() });
  };

  const beginGrip = (event: PointerEvent) => {
    event.stopPropagation();
    if (props.busy || drag() || event.button !== 0 || !stageElement) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLButtonElement;
    handle.focus({ preventScroll: true });
    handle.setPointerCapture(event.pointerId);
    const rect = stageElement.getBoundingClientRect();
    setDrag({
      mode: "grip",
      pointerId: event.pointerId,
      startDist: Math.max(1, Math.hypot(event.clientX - (rect.left + rect.width / 2), event.clientY - (rect.top + rect.height / 2))),
      startZoom: zoom(),
    });
  };

  const beginPan = (event: PointerEvent) => {
    if (props.busy || drag() || event.button !== 0 || !canvasElement || !stageElement) return;
    const scale = canvasElement.getBoundingClientRect().width / canvasSize().width;
    if (scale <= 0) return;
    event.preventDefault();
    stageElement.focus({ preventScroll: true });
    stageElement.setPointerCapture(event.pointerId);
    setDrag({
      mode: "pan",
      pointerId: event.pointerId,
      px: event.clientX,
      py: event.clientY,
      scale,
      pan: { ...pan() },
    });
  };

  const beginRotate = (event: PointerEvent) => {
    event.stopPropagation();
    if (props.busy || drag() || event.button !== 0 || !stageElement) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLButtonElement;
    handle.focus({ preventScroll: true });
    handle.setPointerCapture(event.pointerId);
    const rect = stageElement.getBoundingClientRect();
    setDrag({
      mode: "rotate",
      pointerId: event.pointerId,
      pointerAngle: Math.atan2(event.clientY - (rect.top + rect.height / 2), event.clientX - (rect.left + rect.width / 2)) * 180 / Math.PI,
      startAngle: angle(),
    });
  };

  const beginQuad = (event: PointerEvent, index: number) => {
    event.stopPropagation();
    const points = quad();
    if (props.busy || drag() || event.button !== 0 || !points) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLButtonElement;
    handle.focus({ preventScroll: true });
    handle.setPointerCapture(event.pointerId);
    setDrag({
      mode: "quad",
      pointerId: event.pointerId,
      index,
      startX: event.clientX,
      startY: event.clientY,
      points: points.map(([x, y]) => [x, y] as [number, number]),
    });
  };

  const commitPan = (next: { x: number; y: number }) => {
    pendingPan = next;
    if (!panFrame) panFrame = requestAnimationFrame(flushPan);
  };

  const flushPan = () => {
    if (pendingPan) setPan(pendingPan);
    clearPendingPan();
  };

  const moveDrag = (event: PointerEvent, active = drag()) => {
    if (!active || active.pointerId !== event.pointerId) return;
    if (active.mode === "rotate") {
      if (!stageElement) return;
      const rect = stageElement.getBoundingClientRect();
      const pointerAngle = Math.atan2(event.clientY - (rect.top + rect.height / 2), event.clientX - (rect.left + rect.width / 2)) * 180 / Math.PI;
      const raw = active.startAngle + angleDelta(pointerAngle - active.pointerAngle);
      setAngleValue(event.shiftKey ? Math.round(raw / 15) * 15 : raw);
      return;
    }
    if (active.mode === "scale") {
      const dx = event.clientX - active.startX;
      setZoomValue(active.startZoom * Math.pow(2, dx / 240));
      return;
    }
    if (active.mode === "grip") {
      if (!stageElement) return;
      const rect = stageElement.getBoundingClientRect();
      const distance = Math.hypot(event.clientX - (rect.left + rect.width / 2), event.clientY - (rect.top + rect.height / 2));
      setZoomValue(active.startZoom * (distance / active.startDist));
      return;
    }
    if (active.mode === "quad") {
      const dx = (event.clientX - active.startX) / scale();
      const dy = (event.clientY - active.startY) / scale();
      const points = active.points.map(([x, y], index) =>
        index === active.index ? [Math.round(x + dx), Math.round(y + dy)] as [number, number] : [x, y] as [number, number]);
      setQuad(points);
      return;
    }
    const dx = (event.clientX - active.px) / active.scale;
    const dy = (event.clientY - active.py) / active.scale;
    commitPan({ x: active.pan.x - dx, y: active.pan.y - dy });
  };

  const endDrag = (event: PointerEvent) => {
    const active = drag();
    if (!active || active.pointerId !== event.pointerId) return;
    moveDrag(event, active);
    flushPan();
    setDrag(null);
  };
  const cancelDrag = (event: PointerEvent) => {
    if (drag()?.pointerId !== event.pointerId) return;
    flushPan();
    setDrag(null);
  };

  const nudge = (event: KeyboardEvent) => {
    if (props.busy || drag() || event.target !== stageElement || document.activeElement !== stageElement) return;
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
    event.preventDefault();
    const distance = event.shiftKey ? 10 : 1;
    const { x, y } = pan();
    if (event.key === "ArrowLeft") setPan({ x: x + distance, y });
    if (event.key === "ArrowRight") setPan({ x: x - distance, y });
    if (event.key === "ArrowUp") setPan({ x, y: y + distance });
    if (event.key === "ArrowDown") setPan({ x, y: y - distance });
  };

  const imageStyle = () => {
    const size = canvasSize();
    const box = crop();
    const offsetX = (size.width / 2 - box.x - box.width / 2) * scale();
    const offsetY = (size.height / 2 - box.y - box.height / 2) * scale();
    return {
      width: `${props.source.width * scale()}px`,
      height: `${props.source.height * scale()}px`,
      transform: `translate(calc(-50% + ${offsetX}px), calc(-50% + ${offsetY}px)) rotate(${angle()}deg)`,
    };
  };
  const frameStyle = () => ({
    width: `${frameScreenWidth()}px`,
    height: `${frameScreenWidth() / ratio()}px`,
  });

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
          <span class="section-label">Drag image to position · arrow keys move by 1 px, Shift + arrow by 10 px</span>
          <button type="button" class="btn" classList={{ active: perspective() }} aria-pressed={perspective()}
            disabled={props.busy} onClick={togglePerspective}>Perspective</button>
        </div>
        <div ref={stageElement} class="crop-viewport crop-stage" role="group" tabindex="0" onKeyDown={nudge}
          onPointerDown={beginPan} onPointerMove={moveDrag} onPointerUp={endDrag}
          onPointerCancel={cancelDrag} onLostPointerCapture={cancelDrag}
          aria-label="Scan crop preview. Drag image to position it. Use arrow keys to nudge.">
          <div
            ref={canvasElement}
            class="crop-canvas"
            style={{ width: `${canvasSize().width * scale()}px`, height: `${canvasSize().height * scale()}px` }}
          >
            <div class="rotated-image" style={imageStyle()}>
              <img src={props.source.data_url} alt="Selected calibration scan" draggable={false} />
            </div>
            {perspective() && (
              <>
                <svg class="quad-outline" width={canvasSize().width * scale()} height={canvasSize().height * scale()} aria-hidden="true">
                  <polygon points={quadPoints().map(([x, y]) => `${x * scale()},${y * scale()}`).join(" ")} />
                </svg>
                {quadPoints().map(([x, y], index) => (
                  <button type="button" class="quad-handle"
                    style={{ left: `${x * scale()}px`, top: `${y * scale()}px` }}
                    aria-label={`Perspective corner ${index + 1}`}
                    disabled={props.busy}
                    onPointerDown={(event) => beginQuad(event, index)} />
                ))}
              </>
            )}
          </div>
          {!perspective() && (
            <>
              <div class={outOfBounds() ? "crop-window invalid" : "crop-window"} style={frameStyle()}>
                {props.geometry && <SampleOverlay geometry={props.geometry} />}
              </div>
              <div class="frame-handles" style={frameStyle()}>
                <button type="button" class="frame-grip frame-grip-nw" aria-label="Resize frame from the top left corner" disabled={props.busy} onPointerDown={beginGrip} />
                <button type="button" class="frame-grip frame-grip-ne" aria-label="Resize frame from the top right corner" disabled={props.busy} onPointerDown={beginGrip} />
                <button type="button" class="frame-grip frame-grip-sw" aria-label="Resize frame from the bottom left corner" disabled={props.busy} onPointerDown={beginGrip} />
                <button type="button" class="frame-grip frame-grip-se" aria-label="Resize frame from the bottom right corner" disabled={props.busy} onPointerDown={beginGrip} />
              </div>
            </>
          )}
          <button type="button" class="rotation-handle" aria-label="Rotate image" disabled={props.busy || perspective()}
            onPointerDown={beginRotate}>
            ↻
          </button>
          <div class="zoom-controls" role="group" aria-label="Zoom" onPointerDown={(event) => event.stopPropagation()}>
            <button type="button" aria-label="Zoom in" disabled={props.busy || zoom() >= 20}
              onClick={() => setZoomValue(zoom() * 1.1)}>+</button>
            <button type="button" aria-label="Zoom out" disabled={props.busy || zoom() <= 1}
              onClick={() => setZoomValue(zoom() / 1.1)}>−</button>
            <button type="button" class="zoom-scale" aria-label="Resize image by dragging" title="Drag sideways to resize the image"
              disabled={props.busy} onPointerDown={beginScale}>
              <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
                <rect x="1.5" y="5" width="13" height="6" rx="1" fill="none" stroke="currentColor" />
                <path d="M4 5v3M6.5 5v2M9 5v3M11.5 5v2" stroke="currentColor" fill="none" />
              </svg>
            </button>
          </div>
        </div>
        <div class="crop-fields-head">
          <span class="section-label">Rotated canvas {canvasSize().width} × {canvasSize().height} px · frame {crop().width} × {crop().height} px · zoom {Math.round(zoom() * 100) / 100}×</span>
        </div>
        {perspective() ? (
          <p class="hint">Perspective: drag the four blue corners onto the corners of the engraved gauge. The quadrilateral is straightened to the gauge size, so the rotation control is locked while this is on.</p>
        ) : (
          <>
            <div class="crop-fields crop-readouts">
              <label><span class="field-label">X (px)</span><span class="mono">{crop().x}</span></label>
              <label><span class="field-label">Y (px)</span><span class="mono">{crop().y}</span></label>
              <label><span class="field-label">W (px)</span><span class="mono">{crop().width}</span></label>
              <label><span class="field-label">H (px)</span><span class="mono">{crop().height}</span></label>
              <label><span class="field-label">Rotation (deg)</span><input class="input mono" type="number" min="-180" max="180" step="0.1" disabled={props.busy} value={String(angle())} onInput={(event) => {
                const value = event.currentTarget.valueAsNumber;
                setAngleValue(value);
                if (value < -180 || value > 180) event.currentTarget.value = String(angle());
              }} onBlur={(event) => {
                event.currentTarget.value = String(angle());
              }} /></label>
            </div>
            <p class="hint mono crop-size">{crop().width} × {crop().height} → {props.targetWidth} × {props.targetHeight} px</p>
            {outOfBounds() && <p class="hint warn">The frame extends past the photo. The missing area is filled with black.</p>}
          </>
        )}
        {!props.canImport && <p class="hint warn">This saved scan can be previewed, but saving its crop needs host support.</p>}
        <div class="row-actions">
          <button type="button" class="btn primary" disabled={props.busy || !props.canImport}
            onClick={() => {
              flushPan();
              props.onImport(crop(), angle());
            }}>{props.busy ? (props.editingSaved ? "Saving…" : "Importing…") : (props.editingSaved ? "Save" : "Import")}</button>
          <button type="button" class="btn" disabled={props.busy} onClick={props.onCancel}>Cancel</button>
        </div>
      </section>
    </div>
  );
}

export type { Crop };
