<script lang="ts">
  import { onMount } from "svelte";
  import { Play, ChevronDown, X } from "@lucide/svelte";
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
    publishing = false,
    onsaveversion,
  }: {
    workflowId: string;
    version: Workflow | null;
    devices: Device[];
    device: string;
    changed: boolean;
    onstate: (node: string) => void;
    disabled?: boolean;
    publishing?: boolean;
    onsaveversion: () => void;
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
            error = e instanceof Error ? e.message : "读取试跑记录失败";
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
    if (
      !version ||
      version.id !== workflowId ||
      disabled ||
      restoring ||
      working ||
      (run && busy(run))
    )
      return;
    working = true;
    error = "";
    try {
      run = await api<Run>("/runs", "POST", {
        workflow_id: version.id,
        version: version.version,
        device_id: device,
        trial: true,
      });
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : "试跑失败";
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
    aria-label="版本与试跑"
    aria-haspopup="dialog"
    aria-expanded={open}
    popovertarget="editor-run"
    ><Play size={16} /><span>{run && busy(run) ? runLabel(run) : "试跑"}</span
    ><ChevronDown size={13} /></button
  >
  <div
    id="editor-run"
    class="run-popup"
    bind:this={popup}
    popover="auto"
    role="dialog"
    tabindex="-1"
    aria-label="版本与试跑设置"
    ontoggle={(event) => (open = event.newState === "open")}
  >
    <header>
      <h2>版本与试跑</h2>
      <button
        class="close-button"
        aria-label="关闭试跑设置"
        onclick={() => popup?.hidePopover()}><X size={18} /></button
      >
    </header>
    <p class="version-label">
      {version ? "已保存第 " + version.version + " 版" : "先保存新版本，再试跑"}
    </p>
    <label
      >试跑手机<select
        bind:value={device}
        disabled={disabled || restoring || working || (!!run && busy(run))}
        ><option value="">请选择手机</option>{#each devices as item}<option
            value={item.id}
            disabled={!item.online ||
              (!!version?.devices.length && !version.devices.includes(item.id))}
            >{item.name}{!item.online
              ? "（离线）"
              : version?.devices.length && !version.devices.includes(item.id)
                ? "（未标注）"
                : ""}</option
          >{/each}</select
      ></label
    >
    <div class="run-actions">
      <button
        disabled={disabled ||
          restoring ||
          publishing ||
          working ||
          (!!run && busy(run))}
        onclick={onsaveversion}
        >{publishing ? "正在检查…" : "保存新版本"}</button
      >
      <button
        class="start-button"
        disabled={disabled ||
          restoring ||
          !version ||
          !device ||
          changed ||
          working ||
          (!!run && busy(run))}
        onclick={start}>试跑此版本</button
      >
      {#if run && busy(run)}<button
          disabled={working}
          onclick={() => control(run?.state === "paused" ? "resume" : "pause")}
          >{run.state === "paused" ? "继续" : "暂停"}</button
        ><button disabled={working} onclick={() => control("stop")}>停止</button
        >{/if}
    </div>
    <p>
      {changed && version
        ? "草稿已修改，请重新保存版本后试跑。"
        : version && version.devices.length > 1
          ? "试跑会操作所选手机；标注的手机全部通过后自动启用。请先将手机置于预期页面。"
          : "试跑会操作所选手机；成功后自动启用此版本。请先将手机置于预期页面。"}
    </p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if restoring}<p role="status">正在找回试跑记录…</p>{/if}
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
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
    padding: 18px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 18%);
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
  select {
    width: 100%;
    min-height: 40px;
    font-size: 13px;
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
