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
    <div class={`splash ${props.phase === "leaving" ? "leaving" : ""}`}>
      <div class="splash-inner">
        <div class="splash-mark">
          <span class="tab-mark" aria-hidden="true" />
          <span class="splash-word">engraving</span>
        </div>
        <Show when={props.phase !== "error"} fallback={
          <>
            <p class="splash-error" role="alert">{props.error}</p>
            <button type="button" class="btn" onClick={props.onRetry}>Retry</button>
          </>
        }>
          <span class="splash-msg" role="status">{props.phase === "leaving" ? "Workspace ready" : "Loading workspace…"}</span>
          <div class="splash-bar" aria-hidden="true"><span class="splash-fill" /></div>
        </Show>
      </div>
    </div>
  );
}
