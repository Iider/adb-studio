<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    busy,
    runLabel,
    type Run,
    type Event,
    type Device,
  } from "$lib/api";
  import {
    referencedInputs,
    type Draft,
    type DraftNode,
  } from "$lib/workflow-editor";
  import { inputIssues, parseInputValues } from "$lib/workflow-inputs";
  import InputValues from "./InputValues.svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  let {
    draft,
    node,
    devices,
    onclose,
    onstate,
  }: {
    draft: Draft;
    node: DraftNode;
    devices: Device[];
    onclose: () => void;
    onstate: (id: string) => void;
  } = $props();
  let dialog: HTMLDialogElement;
  const inputs = $derived(referencedInputs([node], draft.inputs ?? []));
  const missing = $derived(inputs.filter((input) => input.default == null));
  let values = $state<Record<string, string>>({});
  let device = $state("");
  let run = $state<Run | null>(null);
  let events = $state<Event[]>([]);
  let working = $state(false);
  let error = $state("");
  const deviceStorageKey = "adb-studio-step-test-device";
  const deviceAvailable = $derived(
    devices.some((item) => item.id === device && item.online),
  );
  const problems = $derived(
    inputIssues(inputs, [node], parseInputValues(inputs, values)),
  );
  async function refresh() {
    if (!run) return;
    const latest = (await api<Run[]>("/runs")).find(
      (item) => item.id === run?.id,
    );
    if (latest) run = latest;
    events = (await api<Event[]>(`/runs/${run.id}/events`)).sort((a, b) =>
      a.at.localeCompare(b.at),
    );
    onstate(busy(run) ? node.id : "");
  }
  async function execute() {
    if (working || (run && busy(run))) return;
    working = true;
    error = "";
    try {
      if (!deviceAvailable) throw new Error("请选择在线手机");
      if (problems.length) throw new Error(problems[0].message);
      run = await api<Run>("/editor/test-step", "POST", {
        draft,
        node_id: node.id,
        device_id: device,
        inputs: parseInputValues(inputs, values),
      });
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : "执行失败";
    } finally {
      working = false;
    }
  }
  async function stop() {
    if (!run) return;
    try {
      await api(`/runs/${run.id}/stop`, "POST");
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : "停止失败";
    }
  }
  function rememberDevice(value: string) {
    try {
      localStorage.setItem(deviceStorageKey, value);
    } catch {
      // 浏览器禁用存储时，仍允许手动选择手机。
    }
  }
  function previousDevice() {
    try {
      const previous = localStorage.getItem(deviceStorageKey);
      return devices.find((item) => item.id === previous && item.online)?.id ?? "";
    } catch {
      return "";
    }
  }
  function prepareAgain() {
    run = null;
    events = [];
    error = "";
  }
  onMount(() => {
    device = previousDevice();
    dialog.showModal();
    const timer = setInterval(() => {
      if (run && busy(run) && !working)
        void refresh().catch((e) => (error = e.message));
    }, 900);
    return () => clearInterval(timer);
  });
</script>

<dialog bind:this={dialog} {onclose} oncancel={onclose} class="step-test">
  <header>
    <h2>执行此步骤 · {node.label}</h2>
    <button onclick={onclose}>关闭</button>
  </header>
  <p>只测试当前步骤，完成后停止。手机动作会真实执行。</p>
  {#if node.kind === "loop"}<p>只检查循环节点参数，不执行循环体。</p>{/if}
  {#if node.config.conditions?.some((condition) => condition.source === "round")}<p
    >
      单步测试按第 1 轮判断。
    </p>{/if}
  {#if !run}
    <label
      >目标手机<Dropdown
        label="单步测试目标手机"
        placeholder="请选择目标手机"
        disabled={working}
        bind:value={device}
        onchange={rememberDevice}
        items={devices.map((item) => ({
          value: item.id,
          label: item.name,
          disabled: !item.online,
        }))}
      /></label
    >
    {#if missing.length}<p>以下变量没有默认值，请填写后确认。</p>
      <InputValues
        inputs={missing}
        nodes={[node]}
        bind:values
        disabled={working}
      />{/if}
    {#if problems.length && !missing.length}<p class="error">
        {problems[0].message}
      </p>{/if}
    <button
      class="primary"
      disabled={working || !deviceAvailable || !!problems.length}
      onclick={execute}>{working ? "正在执行…" : "确认执行"}</button
    >
  {:else}
    <p class="status">{runLabel(run)} · {run.reason}</p>
    {#if busy(run)}<button onclick={stop}>停止测试</button>{:else}<button
        disabled={working}
        onclick={prepareAgain}>再次执行此步骤</button
      >{/if}
    <ol>
      {#each events as event}<li>{event.message}</li>{/each}
    </ol>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</dialog>

<style>
  dialog {
    width: min(520px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    overflow: auto;
    margin: auto;
    padding: 22px;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 14px;
  }
  header {
    display: flex;
    gap: 14px;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    font-size: 17px;
  }
  p,
  li {
    font-size: 13px;
    line-height: 1.7;
    margin: 12px 0;
  }
  p {
    color: var(--dim);
  }
  button {
    border: 1px solid var(--line);
    background: var(--panel);
    color: var(--text);
    padding: 8px 12px;
    border-radius: 8px;
    min-height: 38px;
  }
  button:disabled {
    opacity: 0.5;
  }
  .primary {
    margin-top: 16px;
    background: var(--accent);
    color: var(--primary-ink);
  }
  label {
    margin: 16px 0;
  }
  ol {
    padding-left: 20px;
  }
  .error {
    color: var(--red);
  }
</style>
