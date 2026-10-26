<script lang="ts">
  import { Handle, Position, type NodeProps } from "@xyflow/svelte";
  import {
    ArrowLeft,
    House,
    PanelsTopLeft,
    Clock,
    Crosshair,
    MoveUpRight,
    Repeat2,
    GitBranch,
    Timer,
    LogOut,
    SkipForward,
    Flag,
    Play,
  } from "@lucide/svelte";
  import { ports, summary, type CanvasNode } from "$lib/workflow-editor";
  let { data, selected }: NodeProps<CanvasNode> = $props();
  const icons = {
    back: ArrowLeft,
    home: House,
    recents: PanelsTopLeft,
    wait: Clock,
    tap: Crosshair,
    swipe: MoveUpRight,
    loop: Repeat2,
    condition: GitBranch,
    wait_condition: Timer,
    break: LogOut,
    continue: SkipForward,
    end: Flag,
    entry: Play,
  };
  const Icon = $derived(icons[data.kind]);
  const outputs = $derived(ports(data.kind));
</script>

<div
  class="editor-step"
  class:running={data.running}
  class:chosen={selected}
  class:invalid={!!data.issue}
  class:entry={data.kind === "entry"}
>
  {#if data.kind !== "entry"}<Handle
      type="target"
      position={Position.Top}
      aria-label="连接上一步"
    />{/if}
  <div class="step-title">
    <span class="step-icon"><Icon size={19} /></span><strong
      >{data.label}</strong
    >
  </div>
  <p>{summary(data)}</p>
  {#if data.issue}<small class="step-error">{data.issue}</small>{/if}
  {#if outputs.length}
    <div class="step-ports">
      {#each outputs as port, i (port.id)}
        <span>{port.label}</span>
        <Handle
          type="source"
          id={port.id}
          position={Position.Bottom}
          style={`left: ${((i + 0.5) / outputs.length) * 100}%`}
          aria-label={`连接${port.label}`}
        />
      {/each}
    </div>
  {:else if data.kind === "break" || data.kind === "continue"}
    <Handle
      type="source"
      id="control"
      position={Position.Bottom}
      isConnectable={false}
      aria-label="所属循环控制的去向"
    />
  {/if}
</div>

<style>
  .editor-step {
    width: 230px;
    padding: 15px 16px 0;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  .editor-step.chosen {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accentsoft);
  }
  .editor-step.invalid {
    border-color: var(--red);
  }
  .editor-step.running {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accentsoft);
  }
  .step-title {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 15px;
  }
  .step-title strong {
    overflow-wrap: anywhere;
  }
  .step-icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border-radius: 8px;
    color: var(--accent2);
    background: var(--accentsoft);
  }
  p {
    font-size: 12px;
    line-height: 1.6;
    color: var(--dim);
    margin: 7px 0 12px;
    overflow-wrap: anywhere;
  }
  .step-ports {
    display: flex;
    justify-content: space-around;
    padding: 8px 0;
    border-top: 1px solid var(--line);
    font-size: 11px;
    color: var(--dim);
    gap: 8px;
  }
  .step-error {
    display: block;
    color: var(--red);
    font-size: 12px;
    margin-bottom: 10px;
  }
  .entry {
    width: 150px;
    padding: 12px 14px 0;
    border-radius: 28px;
  }
  .entry .step-icon {
    width: 26px;
    height: 26px;
    border-radius: 50%;
  }
  .entry p {
    display: none;
  }
  .entry .step-ports {
    border: 0;
    padding: 7px 0;
  }
  :global(.workflow-canvas .svelte-flow__handle) {
    width: 12px;
    height: 12px;
    background: var(--panel);
    border: 2px solid var(--accent);
  }
</style>
