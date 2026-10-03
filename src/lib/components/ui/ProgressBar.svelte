<script lang="ts">
  let {
    value,
    label,
    tone = "default",
    class: className = "",
  }: {
    value: number;
    label: string;
    tone?: "default" | "accent" | "warning" | "danger" | "success";
    class?: string;
  } = $props();

  const fillClass = $derived.by(() => {
    if (tone === "accent") return "bg-legio-download";
    if (tone === "warning") return "bg-amber-500";
    if (tone === "danger") return "bg-red-500";
    if (tone === "success") return "bg-emerald-500";
    return "bg-zinc-100 light:bg-zinc-900";
  });
</script>

<div
  class="h-1.5 w-full overflow-hidden rounded-full bg-white/10 light:bg-zinc-900/10 {className}"
  role="progressbar"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={Math.round(value)}
>
  <div
    class="h-full rounded-full transition-[width] duration-300 {fillClass}"
    style="width: {value}%"
  ></div>
</div>
