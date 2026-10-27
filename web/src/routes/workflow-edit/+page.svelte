<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowLeft, Check, Save } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import {
    api,
    session,
    type Device,
    type Node,
    type Workflow,
  } from "$lib/api";

  type Field = {
    key: string;
    label: string;
    kind: "text" | "number" | "checkbox";
    nullable?: boolean;
  };

  const fields: Record<string, Field[]> = {
    fixed_tap_loop: [
      { key: "x", label: "横向位置", kind: "number" },
      { key: "y", label: "纵向位置", kind: "number" },
      { key: "count", label: "点击次数", kind: "number" },
      { key: "interval", label: "间隔（秒）", kind: "number" },
    ],
    swipe_loop: [
      { key: "interval", label: "间隔（秒）", kind: "number" },
      {
        key: "count",
        label: "滑动次数（与运行分钟二选一）",
        kind: "number",
        nullable: true,
      },
      {
        key: "minutes",
        label: "运行分钟（与滑动次数二选一）",
        kind: "number",
        nullable: true,
      },
      { key: "long", label: "长距离滑动", kind: "checkbox" },
    ],
    launch: [
      { key: "package", label: "应用编号", kind: "text" },
      { key: "restart", label: "重新打开应用", kind: "checkbox" },
    ],
    key: [
      { key: "key", label: "按键", kind: "text" },
      { key: "verify", label: "完成后检查", kind: "text" },
    ],
    branch: [{ key: "guard", label: "先确认的页面", kind: "text" }],
    vision_route: [{ key: "package", label: "应用编号", kind: "text" }],
    vision_tap: [
      { key: "package", label: "应用编号", kind: "text" },
      { key: "guard", label: "先确认的页面", kind: "text" },
      { key: "target", label: "点击目标", kind: "text" },
      { key: "verify", label: "完成后检查", kind: "text" },
    ],
    qishui_loop: [
      { key: "warmup", label: "开始等待（秒）", kind: "number" },
      { key: "stall", label: "金币暂停多久后滑动（秒）", kind: "number" },
      { key: "finish_stall", label: "多久无进展后复查（秒）", kind: "number" },
      { key: "finish_swipes", label: "复查滑动次数", kind: "number" },
    ],
    end: [{ key: "result", label: "结束结果", kind: "text" }],
  };
  const kindNames: Record<string, string> = {
    fixed_tap_loop: "重复点击",
    swipe_loop: "重复滑动",
    launch: "打开应用",
    key: "按键",
    branch: "条件分支",
    vision_route: "视觉判断",
    vision_tap: "视觉点击",
    qishui_loop: "汽水刷视频",
    qishui_navigate: "汽水任务入口",
    end: "结束",
    canvas: "画布步骤",
  };

  let workflow = $state<Workflow | null>(null);
  let devices = $state<Device[]>([]);
  let sourceVersion = $state(0);
  let selectedNodeId = $state("");
  let rawStep = $state("");
  let rawDirty = $state(false);
  let sampleText = $state("");
  let samplesDirty = $state(false);
  let initialContent = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let advancedError = $state("");
  let savedVersion = $state(0);

  const selectedNode = $derived(
    workflow?.nodes.find((node) => node.id === selectedNodeId) ?? null,
  );
  const changed = $derived(
    workflow !== null && JSON.stringify(workflow) !== initialContent,
  );
  const stepItems = $derived(
    workflow?.nodes.map((node) => ({ value: node.id, label: node.label })) ??
      [],
  );

  function syncStepText() {
    if (selectedNode && !rawDirty)
      rawStep = JSON.stringify(selectedNode, null, 2);
  }

  function editableFields(node: Node) {
    const values = fields[node.type] ?? [];
    return workflow?.id === "point-click" && node.type === "fixed_tap_loop"
      ? values.filter((field) => !["x", "y"].includes(field.key))
      : values;
  }

  async function load() {
    try {
      await session();
      const id = new URLSearchParams(location.search).get("id");
      if (!id) throw new Error("没有指定要编辑的任务");
      const [versions, availableDevices] = await Promise.all([
        api<Workflow[]>("/workflows"),
        api<Device[]>("/devices"),
      ]);
      const latest = versions
        .filter((item) => item.id === id)
        .sort((a, b) => b.version - a.version)[0];
      if (!latest) throw new Error("任务不存在，请返回任务列表");
      devices = availableDevices;
      workflow = JSON.parse(JSON.stringify(latest));
      sourceVersion = latest.version;
      selectedNodeId = latest.nodes[0]?.id ?? "";
      rawStep = JSON.stringify(latest.nodes[0] ?? {}, null, 2);
      sampleText = JSON.stringify(latest.tests ?? [], null, 2);
      initialContent = JSON.stringify(workflow);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : "无法读取任务";
    } finally {
      loading = false;
    }
  }

  function chooseNode(id: string) {
    if (rawDirty) {
      advancedError = "请先应用或撤销当前步骤的高级修改";
      return;
    }
    selectedNodeId = id;
    advancedError = "";
    syncStepText();
  }

  function updateField(node: Node, field: Field, target: HTMLInputElement) {
    node[field.key] =
      field.kind === "checkbox"
        ? target.checked
        : field.kind === "number"
          ? target.value === "" && field.nullable
            ? null
            : Number(target.value)
          : target.value;
    if (node.type === "swipe_loop" && target.value !== "") {
      if (field.key === "count") node.minutes = null;
      if (field.key === "minutes") node.count = null;
    }
    syncStepText();
  }

  function toggleDevice(id: string, checked: boolean) {
    if (!workflow) return;
    workflow.devices = checked
      ? [...workflow.devices, id]
      : workflow.devices.filter((item) => item !== id);
  }

  function applyStep() {
    if (!workflow || !selectedNode) return;
    try {
      const value = JSON.parse(rawStep) as Node;
      if (!value || Array.isArray(value) || typeof value !== "object")
        throw new Error("步骤内容需要是一个对象");
      if (value.id !== selectedNode.id || value.type !== selectedNode.type)
        throw new Error("步骤编号和类型不能修改");
      workflow.nodes = workflow.nodes.map((node) =>
        node.id === selectedNode.id ? value : node,
      );
      rawDirty = false;
      advancedError = "";
    } catch (cause) {
      advancedError = cause instanceof Error ? cause.message : "步骤格式错误";
    }
  }

  function applySamples() {
    if (!workflow) return;
    try {
      const value = JSON.parse(sampleText);
      if (!Array.isArray(value)) throw new Error("判断样本需要是列表");
      workflow.tests = value;
      samplesDirty = false;
      advancedError = "";
    } catch (cause) {
      advancedError =
        cause instanceof Error ? cause.message : "判断样本格式错误";
    }
  }

  async function save() {
    if (!workflow || saving) return;
    if (rawDirty || samplesDirty) {
      error = "请先应用高级修改，再保存新版本";
      return;
    }
    if (
      !workflow.name.trim() ||
      workflow.nodes.some((node) => !node.label.trim())
    ) {
      error = "请填写任务名称和每个步骤的名称";
      return;
    }
    saving = true;
    error = "";
    try {
      const versions = await api<Workflow[]>("/workflows");
      const latest = Math.max(
        ...versions
          .filter((item) => item.id === workflow?.id)
          .map((item) => item.version),
      );
      if (latest !== sourceVersion)
        throw new Error("这项任务已有新版，请刷新页面后再编辑");
      const candidate = JSON.parse(JSON.stringify(workflow));
      delete candidate.archived;
      candidate.version = sourceVersion + 1;
      candidate.active = false;
      candidate.sample_validated = false;
      const result = await api<Workflow>("/workflows", "POST", candidate);
      workflow = result;
      sourceVersion = result.version;
      savedVersion = result.version;
      initialContent = JSON.stringify(result);
      syncStepText();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : "保存失败，页面内容仍在";
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<svelte:head><title>编辑任务 · ADB 工作台</title></svelte:head>

<main class="revision-page">
  <header class="revision-header">
    <a href="/?tab=workflows" class="back-link"
      ><ArrowLeft size={17} />返回任务</a
    >
    <div>
      <h1>编辑任务</h1>
    </div>
  </header>

  {#if loading}
    <div class="state-message">正在读取任务…</div>
  {:else if !workflow}
    <div class="state-message" role="alert">{error}</div>
  {:else}
    <div class="revision-toolbar">
      <span>当前从第 {sourceVersion} 版修改</span>
      <span>{changed ? "有尚未保存的修改" : "尚未修改"}</span>
      <Button
        disabled={saving || !changed || !!workflow.archived}
        onclick={save}
        ><Save size={16} />{saving ? "正在保存…" : "保存新版本"}</Button
      >
    </div>
    {#if workflow.archived}<p class="notice" role="alert">
        任务已归档，请先恢复再保存新版。
      </p>{/if}
    {#if error}<p class="notice error" role="alert">{error}</p>{/if}
    {#if savedVersion}<p class="notice success" role="status">
        <Check size={16} />第 {savedVersion} 版已保存。
        <a
          href={`/?tab=workflows&workflow=${encodeURIComponent(workflow.id)}&version=${savedVersion}`}
          >前往试跑</a
        >
      </p>{/if}

    <div class="revision-layout">
      <section class="revision-panel" aria-label="任务信息">
        <h2>任务信息</h2>
        <label
          >名称
          <input
            maxlength="80"
            value={workflow.name}
            oninput={(event) => (workflow!.name = event.currentTarget.value)}
          />
        </label>
        <label
          >说明
          <textarea
            rows="3"
            maxlength="2000"
            value={workflow.description}
            oninput={(event) =>
              (workflow!.description = event.currentTarget.value)}></textarea>
        </label>
        <label
          >开始步骤
          <Dropdown
            label="开始步骤"
            value={workflow.entry}
            items={stepItems}
            onchange={(value) => (workflow!.entry = value)}
          />
        </label>
        <label
          >模型调用上限
          <input
            type="number"
            min="0"
            step="1"
            value={workflow.model_limit}
            oninput={(event) =>
              (workflow!.model_limit = Number(event.currentTarget.value))}
          />
        </label>
        <fieldset>
          <legend>适用手机</legend>
          <p>不勾选表示不限手机。</p>
          {#each devices as device (device.id)}
            <label class="device-option"
              ><input
                type="checkbox"
                checked={workflow.devices.includes(device.id)}
                onchange={(event) =>
                  toggleDevice(device.id, event.currentTarget.checked)}
              />{device.name}</label
            >
          {/each}
        </fieldset>
      </section>

      <section class="revision-panel steps-panel" aria-label="任务步骤">
        <h2>任务步骤 <small>共 {workflow.nodes.length} 步</small></h2>
        <div class="steps-layout">
          <nav class="step-list" aria-label="选择要修改的步骤">
            {#each workflow.nodes as node, index (node.id)}
              <button
                class:selected={node.id === selectedNodeId}
                onclick={() => chooseNode(node.id)}
              >
                <span>{index + 1}</span><strong>{node.label}</strong><small
                  >{kindNames[node.type] ?? node.type}</small
                >
              </button>
            {/each}
          </nav>
          {#if selectedNode}
            <div class="step-fields">
              <h3>{kindNames[selectedNode.type] ?? selectedNode.type}</h3>
              <label
                >步骤名称
                <input
                  value={selectedNode.label}
                  oninput={(event) => {
                    selectedNode.label = event.currentTarget.value;
                    syncStepText();
                  }}
                />
              </label>
              {#each editableFields(selectedNode) as field (field.key)}
                <label class:check-field={field.kind === "checkbox"}>
                  {field.label}
                  {#if field.key === "result"}
                    <Dropdown
                      label="结束结果"
                      value={String(selectedNode.result ?? "succeeded")}
                      items={[
                        { value: "succeeded", label: "完成" },
                        { value: "needs_help", label: "需要人工处理" },
                      ]}
                      onchange={(value) => {
                        selectedNode!.result = value;
                        syncStepText();
                      }}
                    />
                  {:else if field.kind === "checkbox"}
                    <input
                      type="checkbox"
                      checked={selectedNode[field.key] === true}
                      onchange={(event) =>
                        updateField(selectedNode, field, event.currentTarget)}
                    />
                  {:else}
                    <input
                      type={field.kind === "number" ? "number" : "text"}
                      step={field.kind === "number" ? "any" : undefined}
                      value={String(selectedNode[field.key] ?? "")}
                      oninput={(event) =>
                        updateField(selectedNode, field, event.currentTarget)}
                    />
                  {/if}
                </label>
              {/each}
              {#if selectedNode.type !== "end"}
                <label
                  >下一步
                  <Dropdown
                    label="下一步"
                    value={selectedNode.next ?? ""}
                    items={[
                      { value: "", label: "无" },
                      ...stepItems.filter(
                        (item) => item.value !== selectedNode.id,
                      ),
                    ]}
                    onchange={(value) => {
                      selectedNode!.next = value || undefined;
                      syncStepText();
                    }}
                  />
                </label>
              {/if}
              {#if workflow.id === "point-click" && selectedNode.type === "fixed_tap_loop"}<p
                >
                  点击位置由设备页标注。
                </p>{/if}
              <details class="advanced">
                <summary>高级参数</summary>
                <p>
                  复杂判断、识别区域和连接关系可在此修改。步骤编号和类型不能改。
                </p>
                <textarea
                  rows="12"
                  spellcheck="false"
                  bind:value={rawStep}
                  oninput={() => (rawDirty = true)}></textarea>
                <div class="advanced-actions">
                  <Button size="sm" onclick={applyStep}>应用修改</Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onclick={() => {
                      rawDirty = false;
                      advancedError = "";
                      syncStepText();
                    }}>撤销输入</Button
                  >
                </div>
              </details>
            </div>
          {/if}
        </div>
      </section>
    </div>

    {#if workflow.tests?.length}
      <details class="revision-panel samples-panel">
        <summary>高级：判断样本</summary>
        <p>修改判断条件时，需要同步核对这些样本。</p>
        <textarea
          rows="10"
          spellcheck="false"
          bind:value={sampleText}
          oninput={() => (samplesDirty = true)}></textarea>
        <div class="advanced-actions">
          <Button size="sm" onclick={applySamples}>应用样本修改</Button>
          <Button
            size="sm"
            variant="outline"
            onclick={() => {
              samplesDirty = false;
              advancedError = "";
              sampleText = JSON.stringify(workflow?.tests ?? [], null, 2);
            }}>撤销输入</Button
          >
        </div>
      </details>
    {/if}
    {#if advancedError}<p class="notice error" role="alert">
        {advancedError}
      </p>{/if}
    <div class="bottom-actions">
      <Button
        disabled={saving || !changed || !!workflow.archived}
        onclick={save}><Save size={16} />保存新版本</Button
      >
      <a href="/?tab=workflows">返回任务列表</a>
    </div>
  {/if}
</main>

<style>
  .revision-page {
    min-height: 100dvh;
    padding: 28px;
    background: var(--bg);
    color: var(--text);
  }
  .revision-header {
    display: flex;
    align-items: flex-start;
    gap: 24px;
    margin: 0 auto 18px;
    max-width: 1380px;
  }
  .revision-header h1 {
    font-size: 26px;
    font-weight: 750;
  }
  .revision-panel p {
    margin-top: 7px;
    color: var(--dim);
    line-height: 1.6;
  }
  .back-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 9px 0;
    white-space: nowrap;
    color: var(--accent2);
    text-decoration: none;
  }
  .revision-toolbar,
  .revision-layout,
  .samples-panel,
  .bottom-actions,
  .state-message,
  .notice {
    max-width: 1380px;
    margin-left: auto;
    margin-right: auto;
  }
  .revision-toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 14px;
    margin-bottom: 18px;
    color: var(--dim);
  }
  .revision-toolbar :global(button) {
    margin-left: auto;
  }
  .revision-layout {
    display: grid;
    grid-template-columns: minmax(280px, 360px) minmax(0, 1fr);
    gap: var(--workspace-gap);
    align-items: start;
  }
  .revision-panel {
    min-width: 0;
    padding: 22px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: var(--panel);
    box-shadow: var(--shadow);
  }
  .revision-panel h2 {
    margin-bottom: 18px;
    font-size: 17px;
  }
  .revision-panel h2 small {
    margin-left: 8px;
    color: var(--dim);
    font-size: 12px;
    font-weight: 500;
  }
  .revision-panel label {
    display: grid;
    gap: 8px;
    margin: 0 0 17px;
    color: var(--text);
    font-weight: 600;
  }
  .revision-panel input:not([type="checkbox"]),
  .revision-panel textarea {
    width: 100%;
    min-height: 40px;
    padding: 10px 11px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel2);
    color: var(--text);
    font: inherit;
    font-weight: 400;
  }
  .revision-panel textarea {
    resize: vertical;
    line-height: 1.5;
  }
  .revision-panel input:focus-visible,
  .revision-panel textarea:focus-visible,
  .step-list button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .revision-panel fieldset {
    padding: 13px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .revision-panel legend {
    padding: 0 4px;
    font-weight: 600;
  }
  .revision-panel fieldset p {
    margin: 0 0 8px;
    font-size: 12px;
  }
  .revision-panel .device-option {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 7px 0;
    font-weight: 400;
  }
  .steps-layout {
    display: grid;
    grid-template-columns: minmax(160px, 230px) minmax(0, 1fr);
    gap: 18px;
  }
  .step-list {
    display: grid;
    align-content: start;
    gap: 7px;
    max-height: 70dvh;
    overflow: auto;
  }
  .step-list button {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr);
    gap: 3px 7px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel2);
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .step-list button.selected {
    border-color: var(--accent);
    background: var(--accentsoft);
  }
  .step-list button span {
    grid-row: span 2;
    color: var(--dim);
  }
  .step-list button strong {
    overflow-wrap: anywhere;
  }
  .step-list button small {
    color: var(--dim);
  }
  .step-fields h3 {
    margin-bottom: 14px;
    font-size: 15px;
  }
  .revision-panel .check-field {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .advanced {
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }
  .advanced summary,
  .samples-panel summary {
    color: var(--accent2);
    font-weight: 650;
    cursor: pointer;
  }
  .advanced textarea,
  .samples-panel textarea {
    margin-top: 12px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px;
  }
  .advanced-actions,
  .bottom-actions {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 12px;
  }
  .samples-panel {
    margin-top: var(--workspace-gap);
  }
  .bottom-actions {
    padding: 18px 0 30px;
  }
  .bottom-actions a,
  .notice a {
    color: var(--accent2);
  }
  .notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    margin-bottom: 18px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel);
  }
  .notice.error {
    color: var(--red);
  }
  .notice.success {
    color: var(--accent2);
  }
  .state-message {
    padding: 32px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: var(--panel);
  }
  @media (max-width: 920px) {
    .revision-layout {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 620px) {
    .revision-page {
      padding: 15px;
    }
    .revision-header {
      display: block;
    }
    .steps-layout {
      grid-template-columns: 1fr;
    }
    .step-list {
      max-height: 230px;
    }
    .revision-toolbar :global(button) {
      width: 100%;
    }
  }
</style>
