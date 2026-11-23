import { For, Show, createSignal } from "solid-js";
import { laserLabel, type Laser } from "./types";

type Props = {
  lasers: Laser[];
  selectedLaser: string | null;
  presetCounts: Record<string, number>;
  busy: boolean;
  onSelect: (id: string) => void;
  onAdd: (name: string, watts: number) => Promise<boolean>;
  onRename: (id: string, name: string, watts: number) => Promise<boolean>;
  onRemove: (id: string) => void;
  onOpenPresets: () => void;
};

export function LasersArea(props: Props) {
  const [name, setName] = createSignal("");
  const [watts, setWatts] = createSignal(0);
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [editName, setEditName] = createSignal("");
  const [editWatts, setEditWatts] = createSignal(0);

  const add = async () => {
    const value = name().trim();
    if (!value || props.busy) return;
    if (await props.onAdd(value, watts())) {
      setName("");
      setWatts(0);
    }
  };

  const startRename = (laser: Laser) => {
    setEditingId(laser.id);
    setEditName(laser.name);
    setEditWatts(laser.watts);
  };

  const commitRename = async (id: string) => {
    const value = editName().trim();
    if (!value) return;
    if (await props.onRename(id, value, editWatts())) setEditingId(null);
  };

  return (
    <section class="area lasers-area" aria-label="Lasers">
      <div class="area-head">
        <div>
          <h1>Lasers</h1>
          <p class="area-note">
            Every preset belongs to one laser, because a calibration is only valid on the machine it was
            measured on.
          </p>
        </div>
      </div>

      <form
        class="laser-add"
        onSubmit={(event) => {
          event.preventDefault();
          void add();
        }}
      >
        <input
          class="input"
          type="text"
          placeholder="Laser name"
          aria-label="New laser name"
          autocomplete="off"
          spellcheck={false}
          disabled={props.busy}
          value={name()}
          onInput={(event) => setName(event.currentTarget.value)}
        />
        <span class="watts-field">
          <input class="input watts-input" type="number" aria-label="Laser watts" min="0" max="1000" step="1" disabled={props.busy} value={watts()} onInput={(event) => setWatts(Number(event.currentTarget.value))} />
          <span class="watts-unit">W</span>
        </span>
        <button type="submit" class="btn" disabled={props.busy || !name().trim()}>
          Add laser
        </button>
      </form>

      <Show
        when={props.lasers.length > 0}
        fallback={
          <div class="empty">
            <span class="empty-mark" aria-hidden="true" />
            <h2>No lasers yet</h2>
            <p>Add your first laser above. Presets cannot exist without a laser.</p>
          </div>
        }
      >
        <ul class="laser-list">
          <For each={props.lasers}>
            {(laser) => (
              <li class={`laser-row ${laser.id === props.selectedLaser ? "selected" : ""}`}>
                <Show
                  when={editingId() === laser.id}
                  fallback={
                    <>
                      <button
                        type="button"
                        class="laser-main"
                        onClick={() => props.onSelect(laser.id)}
                        disabled={props.busy}
                        aria-pressed={laser.id === props.selectedLaser}
                      >
                        <span class="laser-name">{laserLabel(laser)}</span>
                        <span class="laser-count">
                          {props.presetCounts[laser.id] ?? 0} preset
                          {(props.presetCounts[laser.id] ?? 0) === 1 ? "" : "s"}
                        </span>
                      </button>
                      <div class="row-actions">
                        <button
                          type="button"
                          class="btn small"
                          disabled={props.busy}
                          onClick={() => startRename(laser)}
                        >
                          Rename
                        </button>
                        <button
                          type="button"
                          class="btn small danger"
                          disabled={props.busy}
                          onClick={() => props.onRemove(laser.id)}
                        >
                          Remove
                        </button>
                      </div>
                    </>
                  }
                >
                  <input
                    class="input"
                    type="text"
                    aria-label={`Rename ${laserLabel(laser)}`}
                    autocomplete="off"
                    spellcheck={false}
                    disabled={props.busy}
                    value={editName()}
                    onInput={(event) => setEditName(event.currentTarget.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void commitRename(laser.id);
                      if (event.key === "Escape") setEditingId(null);
                    }}
                  />
                  <span class="watts-field">
                    <input class="input watts-input" type="number" aria-label={`Rename ${laserLabel(laser)} watts`} min="0" max="1000" step="1" disabled={props.busy} value={editWatts()} onInput={(event) => setEditWatts(Number(event.currentTarget.value))} />
                    <span class="watts-unit">W</span>
                  </span>
                  <div class="row-actions">
                    <button
                      type="button"
                      class="btn small"
                      disabled={props.busy || !editName().trim()}
                      onClick={() => void commitRename(laser.id)}
                    >
                      Save
                    </button>
                    <button type="button" class="btn small" onClick={() => setEditingId(null)}>
                      Cancel
                    </button>
                  </div>
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <div class="area-foot">
        <p class="hint">
          The selected laser scopes the preset list. A laser that still owns presets cannot be removed; the
          host reports it here.
        </p>
        <button type="button" class="btn small" onClick={props.onOpenPresets} disabled={props.busy}>
          Open presets
        </button>
      </div>
    </section>
  );
}
