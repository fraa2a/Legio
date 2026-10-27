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
    if (tone === "warning") return "bg-amber-500";
    if (tone === "danger") return "bg-red-500";
    if (tone === "success") return "bg-emerald-500";
    return "bg-zinc-100 light:bg-zinc-900";
  });
</script>

<span
  class="block overflow-hidden {variant === 'button' ? 'absolute inset-x-0 bottom-0 h-2 bg-white/30' : 'h-1.5 w-full rounded-full bg-white/10 light:bg-zinc-900/10'} {className}"
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
