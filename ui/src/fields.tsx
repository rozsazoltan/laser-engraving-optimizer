import { Show, createSignal, type JSX } from "solid-js";

type FieldProps = {
  label: string;
  children: JSX.Element;
  span?: boolean;
  flag?: boolean;
  hint?: string;
};

export function Field(props: FieldProps) {
  const [showHint, setShowHint] = createSignal(false);
  return (
    <label class={`field ${props.span ? "span-2" : ""}`}>
      <span class="field-label">
        {props.label}
        <Show when={props.hint}>
          <button
            type="button"
            class="info-btn"
            aria-expanded={showHint()}
            aria-label={`About ${props.label}`}
            title={`About ${props.label}`}
            onClick={(event) => {
              event.preventDefault();
              setShowHint((open) => !open);
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
      <Show when={props.hint && showHint()}>
        <span class="field-hint">{props.hint}</span>
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
        value={props.value}
        aria-label={`${props.label} (${props.min} to ${props.max})`}
        onInput={(event) => {
          const raw = event.currentTarget.value;
          if (raw === "") return;
          const next = Number(raw);
          if (Number.isFinite(next)) props.onValue(next);
        }}
        onBlur={(event) => {
          event.currentTarget.value = String(props.value);
        }}
      />
    </Field>
  );
}
