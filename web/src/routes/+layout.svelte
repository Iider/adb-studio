<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";
  import { installScrollbars } from "$lib/scrollbars";
  import ToastViewport from "$lib/components/ui/toast/toast-viewport.svelte";
  let { children } = $props();
  onMount(() => installScrollbars(document));
</script>

{@render children()}

<ToastViewport />
