import { Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { bridge, errorText } from "./bridge";
import { Chrome } from "./chrome";
import { DocStrip, EngraveArea } from "./engrave";
import { LasersArea } from "./lasers";
import { PresetsArea } from "./presets";
import { Splash, type SplashPhase } from "./splash";
import {
  draftOf,
  laserLabel,
  presetKey,
  type Area,
  type CalibrationSpec,
  type DocTab,
  type Draft,
  type ProcessResult,
  type PresetPreview,
  type ProcessingSettings,
  type State,
} from "./types";

const SPLASH_MIN_MS = 420;
const SPLASH_FADE_MS = 170;

const PHOTO_EXTENSIONS = ["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"];

const WORKING_LABEL: Record<string, string> = {
  "laser-add": "Adding laser",
  "laser-rename": "Renaming laser",
  "select-laser": "Switching laser",
  select: "Switching preset",
  "preset-add": "Creating preset",
  save: "Saving preset",
  "preset-remove": "Removing preset",
  "laser-remove": "Removing laser",
  import: "Importing scan",
  gauge: "Exporting gauge",
  process: "Processing photo",
  "save-engraving": "Saving engraving",
  "save-simulation": "Saving simulation",
  "save-report": "Saving report",
};

const slug = (value: string) =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "engraving";

const versionLabel = (value: string) => {
  const version = value.trim();
  if (!version) return "dev";
  return version.startsWith("v") ? version : `v${version}`;
};

const baseName = (path: string) => path.split(/[\\/]/).pop() || path;

const titleFromPath = (path: string) => baseName(path).replace(/\.[^.]+$/, "") || "Image";

let tabSequence = 0;

function createTab(presetId: string | null): DocTab {
  tabSequence += 1;
  return {
    id: `tab-${Date.now().toString(36)}-${tabSequence}`,
    title: "New image",
    presetId,
    photoPath: null,
    result: null,
    resultKey: null,
  };
}

type Pending = {
  title: string;
  body: string;
  confirm: string;
  danger?: boolean;
  run: () => void;
};

function ConfirmStrip(props: { pending: Pending; onCancel: () => void; onConfirm: () => void }) {
  return (
    <div class="confirm" role="alertdialog" aria-live="assertive" aria-label={props.pending.title}>
      <div class="confirm-text">
        <span class="confirm-mark" aria-hidden="true" />
        <div>
          <strong>{props.pending.title}</strong>
          <p>{props.pending.body}</p>
        </div>
      </div>
      <div class="row-actions">
        <button type="button" class="btn" onClick={props.onCancel}>
          Keep editing
        </button>
        <button
          type="button"
          class={`btn ${props.pending.danger ? "danger-fill" : "primary"}`}
          onClick={props.onConfirm}
        >
          {props.pending.confirm}
        </button>
      </div>
    </div>
  );
}

export default function App() {
  const [data, setData] = createSignal<State | null>(null);
  const [area, setArea] = createSignal<Area>("engrave");
  const [draft, setDraft] = createSignal<Draft | null>(null);
  const [tabs, setTabs] = createSignal<DocTab[]>([]);
  const [activeTabId, setActiveTabId] = createSignal<string | null>(null);
  const [working, setWorking] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [notice, setNotice] = createSignal<string | null>(null);
  const [pending, setPending] = createSignal<Pending | null>(null);
  const [phase, setPhase] = createSignal<SplashPhase | "gone">("loading");
  const [bootError, setBootError] = createSignal("");
  const [previewCache, setPreviewCache] = createSignal<Record<string, PresetPreview>>({});
  const [previewLoading, setPreviewLoading] = createSignal(false);

  let unlisten: (() => void) | null = null;

  const lasers = createMemo(() => data()?.lasers ?? []);

  const selectedLaser = createMemo(() => {
    const state = data();
    return state?.lasers.find((item) => item.id === state.selected_laser) ?? null;
  });

  const laserPresets = createMemo(() => {
    const state = data();
    if (!state) return [];
    return state.presets.filter((item) => item.laser_id === state.selected_laser);
  });

  const selectedPreset = createMemo(() => {
    const state = data();
    if (!state) return null;
    return state.presets.find((item) => item.id === state.selected_preset) ?? null;
  });

  const busy = createMemo(() => working() !== null || data()?.busy === true);

  const activeTab = createMemo(() => {
    const list = tabs();
    const id = activeTabId();
    return list.find((tab) => tab.id === id) ?? list[0] ?? null;
  });

  const tabPreset = createMemo(() => {
    const tab = activeTab();
    if (!tab?.presetId) return null;
    return laserPresets().find((item) => item.id === tab.presetId) ?? null;
  });

  let previewRequest = 0;
  async function refreshPresetPreview(id: string) {
    const request = ++previewRequest;
    setPreviewLoading(true);
    try {
      const preview = await bridge.presetPreview(id);
      setPreviewCache((cache) => ({ ...cache, [id]: preview }));
    } catch {
      // Preview failure must not block editing or replace host status.
    } finally {
      if (request === previewRequest) setPreviewLoading(false);
    }
  }

  createEffect(() => {
    const id = selectedPreset()?.id;
    if (id) void refreshPresetPreview(id);
  });

  const draftDirty = createMemo(() => {
    const current = selectedPreset();
    const edited = draft();
    if (!current || !edited) return false;
    return JSON.stringify(draftOf(current)) !== JSON.stringify(edited);
  });

  const activeError = createMemo(() => failure() ?? data()?.error ?? null);

  const statusText = createMemo(() => {
    const message = activeError();
    if (message) return message;
    const key = working();
    if (key) return `${WORKING_LABEL[key] ?? "Working"}…`;
    return notice() ?? data()?.status ?? "Ready";
  });

  const tabResult = createMemo<ProcessResult | null>(() => {
    const tab = activeTab();
    const preset = tabPreset();
    if (!tab?.result || !tab.resultKey || !preset) return null;
    return presetKey(preset) === tab.resultKey ? tab.result : null;
  });

  const staleReason = createMemo<string | null>(() => {
    const tab = activeTab();
    if (!tab) return null;
    if (!tab.presetId) return "Pick a preset for this tab to start.";
    const preset = tabPreset();
    if (!preset) return "This tab’s preset is not available on the selected laser.";
    if (!preset.has_scan) return `“${preset.name}” has no calibration scan yet. Import one in Presets.`;
    if (!tab.result) return tab.photoPath ? "Process this image with the saved preset." : null;
    if (tab.resultKey !== presetKey(preset)) {
      return "The preset changed since this image was processed. Process it again.";
    }
    if (data()?.selected_preset !== tab.presetId) {
      return "Another preset is selected in this window. Use this tab’s preset before saving.";
    }
    return null;
  });

  const presetSelected = createMemo(() => {
    const tab = activeTab();
    return !!tab?.presetId && data()?.selected_preset === tab.presetId;
  });

  function normalizeTabs(next: State) {
    setTabs((list) =>
      list.map((tab) => {
        if (!tab.presetId) {
          if (!next.selected_preset) return tab;
          return { ...tab, presetId: next.selected_preset, result: null, resultKey: null };
        }
        const found = next.presets.find(
          (item) => item.id === tab.presetId && item.laser_id === next.selected_laser,
        );
        if (found) return tab;
        return { ...tab, presetId: next.selected_preset, result: null, resultKey: null };
      }),
    );
  }

  function adopt(next: State, options: { keepDraft?: boolean } = {}) {
    setData(next);
    if (!options.keepDraft) {
      const current = next.presets.find((item) => item.id === next.selected_preset) ?? null;
      setDraft(current ? draftOf(current) : null);
    }
    normalizeTabs(next);
  }

  async function subscribe() {
    if (unlisten) return;
    try {
      unlisten = await bridge.onState((next) => {
        const previousSelection = data()?.selected_preset;
        setData(next);
        normalizeTabs(next);
        if (!draftDirty() && previousSelection !== next.selected_preset) {
          const current = next.presets.find((item) => item.id === next.selected_preset) ?? null;
          setDraft(current ? draftOf(current) : null);
        }
      });
    } catch {
      unlisten = null;
    }
  }

  async function start() {
    setPhase("loading");
    setBootError("");
    const minimum = new Promise<void>((resolve) => setTimeout(resolve, SPLASH_MIN_MS));
    try {
      const next = await bridge.getState();
      await minimum;
      adopt(next);
      if (tabs().length === 0) {
        const tab = createTab(next.selected_preset);
        setTabs([tab]);
        setActiveTabId(tab.id);
      }
      await subscribe();
      setPhase("leaving");
      setTimeout(() => setPhase("gone"), SPLASH_FADE_MS);
    } catch (error) {
      await minimum;
      setBootError(errorText(error));
      setPhase("error");
    }
  }

  async function run(key: string, task: () => Promise<void>): Promise<boolean> {
    if (busy()) return false;
    setFailure(null);
    setNotice(null);
    setWorking(key);
    try {
      await task();
      return true;
    } catch (error) {
      setFailure(errorText(error));
      return false;
    } finally {
      setWorking(null);
    }
  }

  function updateTab(id: string, patch: Partial<DocTab>) {
    setTabs((list) => list.map((tab) => (tab.id === id ? { ...tab, ...patch } : tab)));
  }

  function confirmPending() {
    const action = pending();
    setPending(null);
    action?.run();
  }

  /** Switch the window-wide preset selection, asking first when the draft is dirty. */
  function switchPreset(presetId: string | null, then?: () => void) {
    if (!presetId || data()?.selected_preset === presetId) {
      then?.();
      return;
    }
    const go = () =>
      void run("select", async () => {
        adopt(await bridge.selectPreset(presetId));
        then?.();
      });
    if (draftDirty()) {
      setPending({
        title: "Unsaved preset changes",
        body: `“${selectedPreset()?.name ?? "The preset"}” has edits that are not saved. Switching discards them.`,
        confirm: "Discard and switch",
        run: go,
      });
      return;
    }
    go();
  }

  // ---- lasers -------------------------------------------------------------

  const addLaser = (name: string, watts: number) =>
    run("laser-add", async () => {
      adopt(await bridge.addLaser(name, watts));
    });

  const renameLaser = (id: string, name: string, watts: number) =>
    run("laser-rename", async () => {
      adopt(await bridge.renameLaser(id, name, watts), { keepDraft: true });
    });

  function removeLaser(id: string) {
    void run("laser-remove", async () => {
      adopt(await bridge.removeLaser(id));
    });
  }

  function selectLaser(id: string) {
    if (id === data()?.selected_laser) return;
    const go = () =>
      void run("select-laser", async () => {
        adopt(await bridge.selectLaser(id));
      });
    if (draftDirty()) {
      setPending({
        title: "Unsaved preset changes",
        body: `“${selectedPreset()?.name ?? "The preset"}” has edits that are not saved. Switching laser discards them.`,
        confirm: "Discard and switch",
        run: go,
      });
      return;
    }
    go();
  }

  // ---- presets ------------------------------------------------------------

  function selectPreset(id: string) {
    if (id === data()?.selected_preset) return;
    switchPreset(id);
  }

  function addPreset() {
    const laserId = data()?.selected_laser;
    if (!laserId) return;
    void run("preset-add", async () => {
      adopt(await bridge.addPreset("Untitled preset", laserId));
    });
  }

  function savePreset() {
    const current = selectedPreset();
    const edited = draft();
    if (!current || !edited) return;
    void run("save", async () => {
      const next = await bridge.savePreset({
        ...current,
        name: edited.name,
        note: edited.note,
        calibration: { ...edited.calibration },
        processing: { ...edited.processing },
        speed: edited.speed,
        power_percent: edited.power_percent,
        dpi: edited.dpi,
      });
      adopt(next);
      const saved = next.presets.find((item) => item.id === current.id);
      if (saved && JSON.stringify(saved.calibration) !== JSON.stringify(current.calibration)) {
        void refreshPresetPreview(saved.id);
      }
    });
  }

  function revertPreset() {
    const current = selectedPreset();
    if (!current) return;
    setDraft(draftOf(current));
    setFailure(null);
    setNotice("Changes reverted");
  }

  function removePreset() {
    const current = selectedPreset();
    if (!current) return;
    setPending({
      title: `Remove “${current.name}”?`,
      body: "The preset and its managed calibration scan are deleted. This cannot be undone.",
      confirm: "Remove preset",
      danger: true,
      run: () =>
        void run("preset-remove", async () => {
          adopt(await bridge.removePreset(current.id));
        }),
    });
  }

  function editText(patch: Partial<Draft>) {
    const current = draft();
    if (!current) return;
    setDraft({ ...current, ...patch });
    setNotice(null);
  }

  function editCalibration(patch: Partial<CalibrationSpec>) {
    const current = draft();
    if (!current) return;
    setDraft({ ...current, calibration: { ...current.calibration, ...patch } });
    setNotice(null);
  }

  function editProcessing(patch: Partial<ProcessingSettings>) {
    const current = draft();
    if (!current) return;
    setDraft({ ...current, processing: { ...current.processing, ...patch } });
    setNotice(null);
  }

  function importScan() {
    const current = selectedPreset();
    if (!current) return;
    void run("import", async () => {
      const path = await bridge.pickOpenFile("Choose calibration scan", ["png", "jpg", "jpeg"]);
      if (!path) return;
      adopt(await bridge.importScan(current.id, path), { keepDraft: true });
      void refreshPresetPreview(current.id);
    });
  }

  function openScanArchive() {
    void run("archive-open", async () => adopt(await bridge.openScanArchive(), { keepDraft: true }));
  }

  function clearScanArchive() {
    void run("archive-clear", async () => adopt(await bridge.clearScanArchive(), { keepDraft: true }));
  }

  function exportGauge() {
    const current = selectedPreset();
    if (!current) return;
    void run("gauge", async () => {
      const path = await bridge.pickSaveFile("Export gauge PNG", `${slug(current.name)}-gauge.png`, "png");
      if (!path) return;
      adopt(await bridge.exportGauge(current.id, path), { keepDraft: true });
    });
  }

  // ---- engrave tabs -------------------------------------------------------

  function activateTab(id: string) {
    const tab = tabs().find((item) => item.id === id);
    if (!tab) return;
    setActiveTabId(id);
    switchPreset(tab.presetId);
  }

  function addTab() {
    const tab = createTab(data()?.selected_preset ?? null);
    setTabs((list) => [...list, tab]);
    setActiveTabId(tab.id);
  }

  function closeTab(id: string) {
    const tab = tabs().find((item) => item.id === id);
    if (!tab) return;

    const apply = () => {
      const remaining = tabs().filter((item) => item.id !== id);
      const nextList = remaining.length > 0 ? remaining : [createTab(data()?.selected_preset ?? null)];
      setTabs(nextList);
      if (activeTabId() === id) {
        const nextActive = nextList[0];
        setActiveTabId(nextActive.id);
        switchPreset(nextActive.presetId);
      }
    };

    if (tab.photoPath || tab.result) {
      setPending({
        title: `Discard “${tab.title}”?`,
        body: "This tab’s chosen image and processed result are dropped. Presets are not affected.",
        confirm: "Discard tab",
        danger: true,
        run: apply,
      });
      return;
    }
    apply();
  }

  function setTabPreset(presetId: string) {
    const tab = activeTab();
    if (!tab) return;
    switchPreset(presetId, () => {
      updateTab(tab.id, { presetId, result: null, resultKey: null });
    });
  }

  function useTabPreset() {
    const tab = activeTab();
    if (tab?.presetId) switchPreset(tab.presetId);
  }

  function chooseImage() {
    const tab = activeTab();
    if (!tab?.presetId) return;
    const preset = tabPreset();
    if (!preset?.has_scan) return;
    void run("process", async () => {
      const path = await bridge.pickOpenFile("Choose photo to engrave", PHOTO_EXTENSIONS);
      if (!path) return;
      if (data()?.selected_preset !== tab.presetId) {
        throw "Select this tab’s preset before processing.";
      }
      updateTab(tab.id, {
        photoPath: path,
        result: null,
        resultKey: null,
        title: tab.title === "New image" ? titleFromPath(path) : tab.title,
      });
      const result = await bridge.processPhoto(tab.id, path);
      const saved = data()?.presets.find((item) => item.id === tab.presetId) ?? null;
      updateTab(tab.id, { result, resultKey: saved ? presetKey(saved) : null });
      setNotice(`Processed ${result.width} × ${result.height} px with “${saved?.name ?? "preset"}”`);
    });
  }

  function reprocessImage() {
    const tab = activeTab();
    if (!tab?.photoPath || !tab.presetId) return;
    void run("process", async () => {
      if (data()?.selected_preset !== tab.presetId) {
        throw "Select this tab’s preset before processing.";
      }
      updateTab(tab.id, { result: null, resultKey: null });
      const result = await bridge.processPhoto(tab.id, tab.photoPath!);
      const saved = data()?.presets.find((item) => item.id === tab.presetId) ?? null;
      updateTab(tab.id, { result, resultKey: saved ? presetKey(saved) : null });
      setNotice(`Processed ${result.width} × ${result.height} px with “${saved?.name ?? "preset"}”`);
    });
  }

  function saveTabResult(kind: "engraving" | "simulation" | "report") {
    const tab = activeTab();
    if (!tab?.result) return;
    if (data()?.selected_preset !== tab.presetId) {
      setFailure("Select this tab’s preset before saving.");
      return;
    }
    const preset = laserPresets().find((item) => item.id === tab.presetId) ?? null;
    const base = slug(preset?.name ?? "engraving");
    const spec =
      kind === "engraving"
        ? { key: "save-engraving", title: "Save engraving PNG", file: `${base}-engraving.png`, ext: "png", verb: "Engraving", call: (path: string) => bridge.saveEngraving(tab.id, path) }
        : kind === "simulation"
          ? { key: "save-simulation", title: "Save simulation PNG", file: `${base}-simulation.png`, ext: "png", verb: "Simulation", call: (path: string) => bridge.saveSimulation(tab.id, path) }
          : { key: "save-report", title: "Save report CSV", file: `${base}-report.csv`, ext: "csv", verb: "Report", call: (path: string) => bridge.saveReport(tab.id, path) };

    void run(spec.key, async () => {
      const path = await bridge.pickSaveFile(spec.title, spec.file, spec.ext);
      if (!path) return;
      adopt(await spec.call(path), { keepDraft: true });
      setNotice(`${spec.verb} saved to ${path}`);
    });
  }

  onMount(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPending(null);
    };
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onKeyDown));
    onCleanup(() => unlisten?.());
    void start();
  });

  return (
    <>
      <div class={`app ${area() === "engrave" ? "with-tabs" : ""}`}>
        <Chrome area={area()} onArea={setArea} />

        <Show when={area() === "engrave"}>
          <DocStrip
            tabs={tabs()}
            activeId={activeTab()?.id ?? null}
            busy={busy()}
            onActivate={activateTab}
            onAdd={addTab}
            onClose={closeTab}
          />
        </Show>

        <main class="surface" aria-busy={busy()}>
          <Show when={data()}>
            {(state) => (
              <>
                <Show when={pending()}>
                  {(action) => (
                    <ConfirmStrip
                      pending={action()}
                      onCancel={() => setPending(null)}
                      onConfirm={confirmPending}
                    />
                  )}
                </Show>

                <Show when={area() === "lasers"}>
                  <LasersArea
                    lasers={state().lasers}
                    selectedLaser={state().selected_laser}
                    presetCounts={state().presets.reduce<Record<string, number>>((acc, item) => {
                      acc[item.laser_id] = (acc[item.laser_id] ?? 0) + 1;
                      return acc;
                    }, {})}
                    busy={busy()}
                    onSelect={selectLaser}
                    onAdd={addLaser}
                    onRename={renameLaser}
                    onRemove={removeLaser}
                    onOpenPresets={() => setArea("presets")}
                  />
                </Show>

                <Show when={area() === "presets"}>
                  <PresetsArea
                    lasers={state().lasers}
                    selectedLaser={state().selected_laser}
                    presets={laserPresets()}
                    preset={selectedPreset()}
                    draft={draft()}
                    dirty={draftDirty()}
                    busy={busy()}
                    working={working()}
                    preview={selectedPreset() ? previewCache()[selectedPreset()!.id] ?? null : null}
                    previewLoading={previewLoading()}
                    onScope={selectLaser}
                    onSelect={selectPreset}
                    onAdd={addPreset}
                    onEdit={editText}
                    onGeometry={editCalibration}
                    onProcessing={editProcessing}
                    onSave={savePreset}
                    onRevert={revertPreset}
                    onRemove={removePreset}
                    onImportScan={importScan}
                    onExportGauge={exportGauge}
                    onOpenArchive={openScanArchive}
                    onClearArchive={clearScanArchive}
                    onOpenLasers={() => setArea("lasers")}
                  />
                </Show>

                <Show when={area() === "engrave"}>
                  <EngraveArea
                    tab={activeTab()}
                    laserName={selectedLaser() ? laserLabel(selectedLaser()!) : null}
                    presets={laserPresets()}
                    preset={tabPreset()}
                    presetSelected={presetSelected()}
                    result={tabResult()}
                    staleReason={staleReason()}
                    busy={busy()}
                    working={working()}
                    hasLasers={state().lasers.length > 0}
                    onPreset={setTabPreset}
                    onUsePreset={useTabPreset}
                    onChoose={chooseImage}
                    onReprocess={reprocessImage}
                    onSave={saveTabResult}
                    onOpenPresets={() => setArea("presets")}
                    onOpenLasers={() => setArea("lasers")}
                  />
                </Show>

                <footer class={`statusbar ${activeError() ? "status-error" : ""}`} role="status" aria-live="polite">
                  <div class="status-left">
                    <span class="status-dot" aria-hidden="true" />
                    <span class="status-text" title={statusText()}>
                      {statusText()}
                    </span>
                  </div>
                  <div class="status-right">
                    <Show when={draftDirty()}>
                      <span class="status-dirty">unsaved preset</span>
                    </Show>
                    <span>{versionLabel(state().app_version)}</span>
                  </div>
                </footer>
              </>
            )}
          </Show>
        </main>
      </div>

      <Show when={phase() !== "gone"}>
        <Splash
          phase={phase() === "gone" ? "leaving" : (phase() as SplashPhase)}
          error={bootError()}
          onRetry={() => void start()}
        />
      </Show>
    </>
  );
}
