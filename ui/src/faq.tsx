import { For } from "solid-js";

const entries = [
  {
    title: "First row stays white",
    text: "The light end sits below the laser threshold. Raise max power by 5-10%, or lower the speed by about 20%, then engrave again. Keep DPI at 254 or above and check that the focus is on the material.",
  },
  {
    title: "Gauge looks charred",
    text: "Lower max power (25% to 18-20%) or raise the speed (50 to 70-80 mm/s), and increase air assist. The darkest patch should be dark brown, not black soot, and must not have a raised crust.",
  },
  {
    title: "Finished engraving looks burnt",
    text: "The curve comes from the gauge, so a charred gauge makes every result charred. If the fitted darkness maximum is close to 230-255 there is no headroom left: recalibrate with lighter settings. For portraits try gamma 1.2-1.6 and CLAHE 1.0-2.0.",
  },
  {
    title: "Want an even brown instead of charred",
    text: "Start at 318 DPI, 70-90 mm/s and 18-22% on a 22 W diode over 3 mm plywood, strong air assist, one pass. A slight defocus (+0.5-1 mm) gives a browner surface; keep that focus for production work afterwards.",
  },
  {
    title: "Before the scan photo",
    text: "Brush off loose soot with a dry cloth or brush, always the same way. Loose soot reads darker than the surface and skews the curve.",
  },
  {
    title: "LightBurn",
    text: "Use image mode Grayscale, never Dither or Threshold. Keep invert off, leave brightness, contrast, gamma and enhance untouched, set the line interval to the recorded DPI, min power 0, one pass and overscan on.",
  },
  {
    title: "Which settings a preset covers",
    text: "A calibration is only valid for the speed, power percentage and DPI it was engraved with. Record those in the preset; a different combination needs its own gauge.",
  },
];

export function FaqArea() {
  return (
    <section class="area faq-area" aria-label="FAQ">
      <header class="area-head">
        <div><h1>FAQ</h1></div>
      </header>
      <dl class="faq-list">
        <For each={entries}>
          {(entry) => (
            <div class="faq-entry">
              <dt>{entry.title}</dt>
              <dd>{entry.text}</dd>
            </div>
          )}
        </For>
      </dl>
    </section>
  );
}
