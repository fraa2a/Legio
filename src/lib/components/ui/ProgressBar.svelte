<script lang="ts">
  let {
    value,
    label,
    tone = "default",
    variant = "default",
    class: className = "",
  }: {
    value: number;
    label: string;
    tone?: "default" | "accent" | "warning" | "danger" | "success";
    class?: string;
    variant?: "default" | "button";
  } = $props();

  const percent = $derived(Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : 0);
  const fillClass = $derived.by(() => {
    if (variant === "button") return "bg-white";
    if (tone === "accent") return "bg-legio-download";
    if (tone === "warning") return "bg-legio-warning";
    if (tone === "danger") return "bg-legio-error";
    if (tone === "success") return "bg-legio-success";
    return "bg-legio-fg";
  });
</script>

<span
  class="block overflow-hidden {variant === 'button' ? 'absolute inset-x-0 bottom-0 h-2 bg-white/30' : 'h-1.5 w-full rounded-full bg-legio-fill-hover'} {className}"
  role="progressbar"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={Math.round(percent)}
>
  <span
    class="block h-full transition-[width] duration-300 motion-reduce:transition-none {variant === 'default' ? 'rounded-full' : ''} {fillClass}"
    style:width={`${percent}%`}
  ></span>
</span>
