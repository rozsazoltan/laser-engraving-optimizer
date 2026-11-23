import { Show, createEffect, createSignal, onCleanup } from "solid-js";

export type SplashPhase = "loading" | "error" | "leaving";

type Props = {
  phase: SplashPhase;
  error: string;
  onRetry: () => void;
};

/** How fast the determinate bar creeps towards 95% while the first state loads. */
const STEP = 3;
const STEP_MS = 40;
const HOLD = 95;

/** Startup screen: shown until the first state arrives, or with a retry on failure. */
export function Splash(props: Props) {
  const [progress, setProgress] = createSignal(0);

  createEffect(() => {
    if (props.phase !== "loading") return;
    const timer = window.setInterval(() => setProgress((value) => Math.min(HOLD, value + STEP)), STEP_MS);
    onCleanup(() => window.clearInterval(timer));
  });

  createEffect(() => {
    if (props.phase === "leaving") setProgress(100);
  });

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
              <span class="splash-msg">{progress()}%</span>
              <div
                class="splash-bar"
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={progress()}
              >
                <span class="splash-fill" style={{ width: `${progress()}%` }} />
              </div>
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
