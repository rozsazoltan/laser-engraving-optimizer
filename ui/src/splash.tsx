import { Show } from "solid-js";

export type SplashPhase = "loading" | "error" | "leaving";

type Props = {
  phase: SplashPhase;
  error: string;
  onRetry: () => void;
};

/** Startup screen: shown until the first state arrives, or with a retry on failure. */
export function Splash(props: Props) {
  return (
    <div class={`splash ${props.phase === "leaving" ? "leaving" : ""}`} role="status" aria-live="polite">
      <div class="splash-inner">
        <div class="splash-mark">
          <span class="tab-mark" aria-hidden="true" />
          <span class="splash-word">engraving</span>
        </div>

        <Show
          when={props.phase === "error"}
          fallback={
            <>
              <span class="splash-msg">Loading presets…</span>
              <div class="splash-bar" aria-hidden="true" />
            </>
          }
        >
          <p class="splash-error">{props.error}</p>
          <button type="button" class="btn" onClick={props.onRetry}>
            Retry
          </button>
        </Show>
      </div>
    </div>
  );
}
