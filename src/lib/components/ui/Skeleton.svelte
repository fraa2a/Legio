<script lang="ts">
  let { class: className = "" }: { class?: string } = $props();
</script>

<span class="block animate-pulse rounded-xl bg-legio-fill-hover motion-reduce:animate-none {className}" aria-hidden="true"></span>
