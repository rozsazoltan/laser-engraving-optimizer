import { Show, createEffect, createSignal, onCleanup, onMount, type JSX } from "solid-js";

type FieldProps = {
  label: string;
  children: JSX.Element;
  span?: boolean;
  flag?: boolean;
  hint?: string;
};

export function Field(props: FieldProps) {
  const [pinned, setPinned] = createSignal(false);
  const [hovered, setHovered] = createSignal(false);
  const [position, setPosition] = createSignal({ left: 0, top: 0 });
  const visible = () => pinned() || hovered();
  const id = `field-tooltip-${Math.random().toString(36).slice(2)}`;
  let button: HTMLButtonElement | undefined;
  const placeTooltip = () => {
    if (!button) return;
    const rect = button.getBoundingClientRect();
    const width = Math.min(320, window.innerWidth - 24);
    const left = Math.max(12, Math.min(rect.left, window.innerWidth - width - 12));
    const height = Math.min(180, Math.ceil((props.hint?.length ?? 0) / 42) * 17 + 20);
    const top = rect.bottom + height + 8 < window.innerHeight ? rect.bottom + 8 : Math.max(12, rect.top - height - 8);
    setPosition({ left, top });
  };
  const closeOutside = (event: PointerEvent) => {
    if (button && !button.contains(event.target as Node)) setPinned(false);
  };
  const closeEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      setPinned(false);
      setHovered(false);
    }
  };
  onMount(() => {
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeEscape);
    onCleanup(() => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeEscape);
    });
  });
  return (
    <label class={`field ${props.span ? "span-2" : ""}`}>
      <span class="field-label">
        {props.label}
        <Show when={props.hint}>
          <button
            ref={button}
            type="button"
            class="info-btn"
            aria-expanded={visible()}
            aria-describedby={id}
            aria-label={`About ${props.label}`}
            title={`About ${props.label}`}
            onMouseEnter={() => { setHovered(true); placeTooltip(); }}
            onMouseLeave={() => setHovered(false)}
            onFocus={() => { setHovered(true); placeTooltip(); }}
            onBlur={() => setHovered(false)}
            onClick={(event) => {
              event.preventDefault();
              placeTooltip();
              setPinned((open) => !open);
            }}
          >
            i
          </button>
        </Show>
        <Show when={props.flag}>
          <b class="dirty-flag">unsaved</b>
        </Show>
      </span>
      {props.children}
      <Show when={props.hint && visible()}>
        <span id={id} class="field-tooltip" role="tooltip" style={{ left: `${position().left}px`, top: `${position().top}px` }}>{props.hint}</span>
      </Show>
    </label>
  );
}

type NumberFieldProps = {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  disabled: boolean;
  hint?: string;
  onValue: (value: number) => void;
};

export function NumberField(props: NumberFieldProps) {
  const [text, setText] = createSignal(String(props.value));
  let input: HTMLInputElement | undefined;
  createEffect(() => {
    const value = props.value;
    if (document.activeElement !== input) setText(String(value));
  });
  return (
    <Field label={props.label} hint={props.hint}>
      <input
        class="input mono"
        type="number"
        inputmode="decimal"
        min={props.min}
        max={props.max}
        step={props.step}
        disabled={props.disabled}
        ref={input}
        value={text()}
        aria-label={`${props.label} (${props.min} to ${props.max})`}
        onInput={(event) => {
          const raw = event.currentTarget.value;
          setText(raw);
          if (raw === "") return;
          const next = Number(raw);
          if (Number.isFinite(next)) props.onValue(next);
        }}
        onBlur={(event) => {
          const raw = event.currentTarget.value;
          const next = Number(raw);
          if (raw !== "" && Number.isFinite(next)) {
            props.onValue(next);
            setText(raw);
          } else {
            setText(String(props.value));
          }
        }}
      />
    </Field>
  );
}
