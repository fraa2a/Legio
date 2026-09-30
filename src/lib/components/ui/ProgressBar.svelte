<script lang="ts">
  let {
    value,
    label,
    class: className = "",
  }: {
    value: number;
    label: string;
    class?: string;
  } = $props();
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
    class="h-full rounded-full bg-zinc-100 transition-[width] duration-300 light:bg-zinc-900"
    style="width: {value}%"
  ></div>
</div>
