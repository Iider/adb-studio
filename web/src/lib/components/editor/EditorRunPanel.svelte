<script lang="ts">
  import { onMount } from "svelte";
  import { Play, ChevronDown, X } from "@lucide/svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import {
    api,
    busy,
    runLabel,
    type Workflow,
    type Device,
    type Run,
    type Event,
  } from "$lib/api";
  let {
    workflowId,
    version,
    devices,
    device = $bindable(""),
    changed,
    onstate,
    disabled = false,
    savingVersion = false,
    onsaveversion,
  }: {
    workflowId: string;
    version: Workflow | null;
    devices: Device[];
    device: string;
    changed: boolean;
    onstate: (node: string) => void;
    disabled?: boolean;
    savingVersion?: boolean;
    onsaveversion: () => Promise<Workflow | null>;
  } = $props();
  let run = $state<Run | null>(null);
  let events = $state<Event[]>([]);
  let working = $state(false);
  let restoring = $state(false);
  let error = $state("");
  let popup = $state<HTMLElement>();
  let open = $state(false);
  $effect(() => {
    const allowed = (item: Device) =>
      !version?.devices.length || version.devices.includes(item.id);
    if (
      (!device ||
        !devices.some((item) => item.id === device && allowed(item))) &&
      !(run && busy(run))
    )
      device = devices.find((item) => item.online && allowed(item))?.id ?? "";
  });
  $effect(() => {
    const currentWorkflow = workflowId;
    let cancelled = false;
    run = null;
    events = [];
    error = "";
    restoring = !!currentWorkflow;
    if (currentWorkflow)
      void api<Run[]>("/runs")
        .then(async (rows) => {
          if (cancelled) return;
          run =
            rows
              .filter((item) => item.workflow.id === currentWorkflow)
              .sort(
                (a, b) =>
                  Number(busy(b)) - Number(busy(a)) ||
                  b.started_at.localeCompare(a.started_at),
              )[0] ?? null;
          if (run) {
            if (busy(run)) device = run.device_id;
            await refresh();
          }
        })
        .catch((e) => {
          if (!cancelled)
            error = e instanceof Error ? e.message : "读取运行记录失败";
        })
        .finally(() => {
          if (!cancelled) restoring = false;
        });
    return () => {
      cancelled = true;
    };
  });
  async function refresh() {
    const tracked = run,
      currentWorkflow = workflowId;
    if (!tracked) return;
    try {
      const rows = await api<Run[]>("/runs");
      const latest = rows.find((item) => item.id === tracked.id);
      if (!latest) {
        if (workflowId === currentWorkflow && run?.id === tracked.id) {
          run = null;
          events = [];
          onstate("");
        }
        return;
      }
      const logs = (await api<Event[]>("/runs/" + tracked.id + "/events")).sort(
        (a, b) => a.at.localeCompare(b.at),
      );
      if (workflowId !== currentWorkflow || run?.id !== tracked.id) return;
      run = latest;
      events = logs;
      onstate(busy(latest) ? latest.node : "");
      error = "";
    } catch (e) {
      if (workflowId === currentWorkflow && run?.id === tracked.id)
        error = e instanceof Error ? e.message : "读取进度失败";
    }
  }
  async function start() {
    if (disabled || restoring || working || (run && busy(run))) return;
    working = true;
    error = "";
    try {
      const saved = !version || changed ? await onsaveversion() : version;
      if (!saved || saved.id !== workflowId) return;
      run = await api<Run>("/runs", "POST", {
        workflow_id: saved.id,
        version: saved.version,
        device_id: device,
      });
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : "运行失败";
    } finally {
      working = false;
    }
  }
  async function control(operation: string) {
    if (!run) return;
    working = true;
    error = "";
    try {
      await api("/runs/" + run.id + "/" + operation, "POST");
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : "操作失败";
    } finally {
      working = false;
    }
  }
  onMount(() => {
    const timer = setInterval(() => {
      if (run && busy(run) && !working) void refresh();
    }, 1200);
    return () => clearInterval(timer);
  });
</script>

<div class="run-control">
  <button
    class="run-trigger"
    class:attention={!!error}
    {disabled}
    aria-label="版本与运行"
    aria-haspopup="dialog"
    aria-expanded={open}
    popovertarget="editor-run"
    ><Play size={16} /><span>{run && busy(run) ? runLabel(run) : "运行"}</span
    ><ChevronDown size={13} /></button
  >
  <div
    id="editor-run"
    class="run-popup"
    bind:this={popup}
    popover="auto"
    role="dialog"
    tabindex="-1"
    aria-label="版本与运行设置"
    ontoggle={(event) => (open = event.newState === "open")}
  >
    <header>
      <h2>版本与运行</h2>
      <button
        class="close-button"
        aria-label="关闭运行设置"
        onclick={() => popup?.hidePopover()}><X size={18} /></button
      >
    </header>
    <p class="version-label">
      {version ? "已保存第 " + version.version + " 版" : "运行时保存新版本"}
    </p>
    <label
      >运行手机<Dropdown
        label="运行手机"
        bind:value={device}
        disabled={disabled || restoring || working || (!!run && busy(run))}
        items={[
          { value: "", label: "请选择手机" },
          ...devices.map((item) => ({
            value: item.id,
            label: `${item.name}${
              !item.online
                ? "（离线）"
                : version?.devices.length && !version.devices.includes(item.id)
                  ? "（未标注）"
                  : ""
            }`,
            disabled:
              !item.online ||
              (!!version?.devices.length && !version.devices.includes(item.id)),
          })),
        ]}
      /></label
    >
    <div class="run-actions">
      <button
        disabled={disabled ||
          restoring ||
          savingVersion ||
          working ||
          (!!run && busy(run))}
        onclick={onsaveversion}
        >{savingVersion ? "正在保存…" : "保存新版本"}</button
      >
      <button
        class="start-button"
        disabled={disabled ||
          restoring ||
          savingVersion ||
          !device ||
          working ||
          (!!run && busy(run))}
        onclick={start}>运行</button
      >
      {#if run && busy(run)}<button
          disabled={working}
          onclick={() => control(run?.state === "paused" ? "resume" : "pause")}
          >{run.state === "paused" ? "继续" : "暂停"}</button
        ><button disabled={working} onclick={() => control("stop")}>停止</button
        >{/if}
    </div>
    <p>
      {!version || changed
        ? "运行时自动保存当前修改。"
        : ""}请先将手机置于预期页面。
    </p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if restoring}<p role="status">正在找回运行记录…</p>{/if}
    {#if run}<details>
        <summary
          >第 {run.workflow.version} 版 · {runLabel(run)} · {run.reason}</summary
        >
        <ol>
          {#each events.slice(-30) as event}<li>{event.message}</li>{/each}
        </ol>
      </details>{/if}
  </div>
</div>

<style>
  .run-control {
    display: inline-flex;
    flex-shrink: 0;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid var(--line);
    background: var(--panel);
    color: var(--text);
    border-radius: 8px;
    padding: 8px 11px;
    height: 38px;
    white-space: nowrap;
    font-size: 13px;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .run-trigger.attention {
    color: var(--red);
    border-color: var(--red);
  }
  .run-trigger span {
    max-width: 88px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .run-popup {
    position: fixed;
    inset: 70px 16px auto auto;
    margin: 0;
    width: min(420px, calc(100vw - 32px));
    max-height: calc(100dvh - 86px);
    overflow: auto;
    scrollbar-gutter: stable;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
    padding: 18px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  h2 {
    font-size: 16px;
  }
  .close-button {
    width: 32px;
    height: 32px;
    padding: 5px;
    border-color: transparent;
  }
  .version-label {
    color: var(--text);
    font-weight: 600;
  }
  label {
    font-size: 13px;
    margin: 16px 0;
  }
  .run-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .start-button {
    background: var(--accent);
    color: var(--primary-ink);
    border-color: var(--accent);
  }
  p {
    font-size: 12px;
    color: var(--dim);
    margin-top: 12px;
    line-height: 1.7;
  }
  .error {
    color: var(--red);
  }
  details {
    margin-top: 14px;
    max-height: 220px;
    overflow: auto;
    scrollbar-gutter: stable;
    font-size: 12px;
  }
  summary,
  li {
    line-height: 1.8;
  }
  ol {
    padding-left: 20px;
  }
  @media (max-width: 600px) {
    .run-trigger {
      padding: 8px;
    }
    .run-trigger :global(svg:last-child) {
      display: none;
    }
  }
</style>
