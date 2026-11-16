<script lang="ts">
  let {
    id,
    label,
    value,
    min,
    max,
    step = 1,
    disabled = false,
    valueText,
    buffered = 0,
    tone = "default",
    onInput,
    onCommit,
    onPointerDown,
    class: className = "",
  }: {
    id?: string;
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    disabled?: boolean;
    valueText?: string;
    // Loaded share of a media track, from 0 to 100.
    buffered?: number;
    // Media sliders sit on video, so their track stays white in every theme.
    tone?: "default" | "media";
    onInput: (value: number) => void;
    onCommit?: () => void;
    onPointerDown?: () => void;
    class?: string;
  } = $props();

  const filled = $derived(max > min ? Math.min(100, Math.max(0, (value - min) / (max - min) * 100)) : 0);
</script>

<input
  {id}
  type="range"
  {min}
  {max}
  {step}
  {value}
  {disabled}
  aria-label={label}
  aria-valuetext={valueText}
  style:--filled="{filled}%"
  style:--buffered="{buffered}%"
  class="slider {className}"
  class:slider-media={tone === "media"}
  onpointerdown={onPointerDown}
  onchange={onCommit}
  oninput={(event) => onInput(Number(event.currentTarget.value))}
/>

<style>
  .slider {
    --track: var(--legio-fill-active);
    --track-loaded: var(--legio-line-strong);
    --thumb: var(--legio-text);
    appearance: none;
    height: 1.25rem;
    background: transparent;
    cursor: pointer;
  }
  .slider-media {
    --track: rgb(255 255 255 / 0.22);
    --track-loaded: rgb(255 255 255 / 0.5);
    --thumb: white;
  }
  .slider:disabled { cursor: not-allowed; opacity: 0.5; }
  .slider::-webkit-slider-runnable-track {
    height: 0.25rem;
    border-radius: 9999px;
    background:
      linear-gradient(to right, var(--legio-accent) var(--filled), transparent var(--filled)),
      linear-gradient(to right, var(--track-loaded) var(--buffered), var(--track) var(--buffered));
  }
  .slider::-moz-range-track {
    height: 0.25rem;
    border-radius: 9999px;
    background:
      linear-gradient(to right, var(--legio-accent) var(--filled), transparent var(--filled)),
      linear-gradient(to right, var(--track-loaded) var(--buffered), var(--track) var(--buffered));
  }
  .slider::-webkit-slider-thumb {
    appearance: none;
    width: 0.75rem;
    height: 0.75rem;
    margin-top: -0.25rem;
    border-radius: 9999px;
    background: var(--thumb);
  }
  .slider::-moz-range-thumb {
    width: 0.75rem;
    height: 0.75rem;
    border: 0;
    border-radius: 9999px;
    background: var(--thumb);
  }
</style>
