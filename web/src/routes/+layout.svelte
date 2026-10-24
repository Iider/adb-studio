<script lang="ts">
  import "../app.css";
  import ToastViewport from "$lib/components/ui/toast/toast-viewport.svelte";
  let { children } = $props();
</script>

{@render children()}

<ToastViewport />
