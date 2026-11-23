import { Show, type JSX } from "solid-js";

type FieldProps = {
  label: string;
  children: JSX.Element;
  span?: boolean;
  flag?: boolean;
};

export function Field(props: FieldProps) {
  return (
    <label class={`field ${props.span ? "span-2" : ""}`}>
      <span class="field-label">
        {props.label}
        <Show when={props.flag}>
          <b class="dirty-flag">unsaved</b>
        </Show>
      </span>
      {props.children}
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
  onValue: (value: number) => void;
};

export function NumberField(props: NumberFieldProps) {
  return (
    <Field label={props.label}>
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
