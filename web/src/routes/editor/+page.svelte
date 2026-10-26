<script lang="ts">
  import { SvelteFlowProvider } from "@xyflow/svelte";
  import WorkflowEditor from "$lib/components/editor/WorkflowEditor.svelte";
  import "@xyflow/svelte/dist/style.css";
</script>

<svelte:head><title>编排工作流 · ADB 工作台</title></svelte:head>
<SvelteFlowProvider><WorkflowEditor /></SvelteFlowProvider>
