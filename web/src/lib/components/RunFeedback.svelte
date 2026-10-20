<script lang="ts">
  import { runFeedback } from "$lib/run-feedback";
  let { message, detail = null }: { message: string; detail?: string | null } =
    $props();
  const feedback = $derived(runFeedback(message, detail));
</script>

<p>{feedback.summary}</p>
{#if feedback.detail}<details class="run-technical">
    <summary>技术详情</summary>
    <pre>{feedback.detail}</pre>
  </details>{/if}
