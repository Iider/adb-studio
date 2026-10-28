<script lang="ts">
  import { onMount, tick } from "svelte";
  import { graphlib, layout } from "@dagrejs/dagre";
  import {
    SvelteFlow,
    Background,
    Controls,
    useSvelteFlow,
    type Viewport,
  } from "@xyflow/svelte";
  import {
    ArrowLeft,
    Plus,
    Save,
    Copy,
    Download,
    Trash2,
    Undo2,
    Redo2,
    Scan,
    Check,
    Workflow,
    Moon,
    Sun,
    AlertCircle,
    RefreshCw,
    MoreHorizontal,
    X,
  } from "@lucide/svelte";
  import {
    api,
    session,
    time,
    type Device,
    type Workflow as WorkflowDefinition,
  } from "$lib/api";
  import { notify } from "$lib/notifications";
  import {
    blankDraft,
    toCanvas,
    edgesFor,
    palette,
    ports,
    defaults,
    checkGraph,
    canonical,
    actionLimit,
    type DraftNode,
    type Draft,
    type Kind,
    type CanvasNode,
    type Config,
    type Issue,
  } from "$lib/workflow-editor";
  import StepNode from "./StepNode.svelte";
  import StepInspector from "./StepInspector.svelte";
  import CoordinatePicker from "./CoordinatePicker.svelte";
  import EditorRunPanel from "./EditorRunPanel.svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import WorkflowExport from "$lib/components/WorkflowExport.svelte";

  const nodeTypes = { step: StepNode };
  const flow = useSvelteFlow<CanvasNode>();
  let drafts = $state<Draft[]>([]);
  let devices = $state<Device[]>([]);
  let deviceId = $state("");
  let markNode = $state<CanvasNode | null>(null);
  let markCondition = $state<number | null>(null);
  let savedWorkflow = $state<WorkflowDefinition | null>(null);
  let workflowContent = $state("");
  let savingWorkflow = $state(false);
  let executionError = $state("");
  let id = $state("");
  let draftChoice = $state("");
  let revision = $state(0);
  let name = $state("");
  let description = $state("");
  let entry = $state("");
  let nodes = $state.raw<CanvasNode[]>([]);
  let viewport = $state<Viewport>({ x: 30, y: 25, zoom: 0.9 });
  let selected = $state("");
  let selectedEdge = $state("");
  let ready = $state(false);
  let loadingError = $state("");
  let saveError = $state("");
  let saving = $state(false);
  let switching = $state(false);
  let savedContent = $state("");
  let updatedAt = $state("");
  let recovery = $state<Draft | null>(null);
  let exportRequest = $state<{ path: string; name: string } | null>(null);
  let history = $state<string[]>([]);
  let historyIndex = $state(0);
  let issues = $state<Issue[]>([]);
  let checksOpen = $state(false);
  let search = $state("");
  let panel = $state<"palette" | "settings">("palette");
  let dark = $state(false);
  let deleteDialog = $state<HTMLDialogElement>();
  let canvas = $state<HTMLDivElement>();
  let morePopover = $state<HTMLElement>();
  let problemsPopover = $state<HTMLElement>();
  let moreOpen = $state(false);
  let problemsOpen = $state(false);
  const hasProblems = $derived(!!executionError || !!saveError || !!recovery);
  $effect(() => {
    if (ready && (executionError || saveError || recovery))
      problemsPopover?.showPopover();
  });
  const edges = $derived(edgesFor(nodes, entry, selectedEdge));
  const current = $derived(nodes.find((node) => node.id === selected));
  const content = $derived(
    JSON.stringify({
      id,
      name,
      description,
      entry,
      nodes: nodes
        .filter((node) => node.id !== "__entry")
        .map((node) => ({
          id: node.id,
          kind: node.data.kind,
          label: node.data.label,
          config: node.data.config,
          targets: node.data.targets,
          position: [node.position.x, node.position.y],
        })),
      viewport: [viewport.x, viewport.y, viewport.zoom],
    }),
  );
  const dirty = $derived(content !== savedContent);
  const executionContent = $derived(
    canonical({
      name,
      description,
      entry,
      nodes: nodes
        .filter((node) => node.id !== "__entry")
        .map((node) => ({
          id: node.id,
          kind: node.data.kind,
          label: node.data.label,
          config: node.data.config,
          targets: node.data.targets,
        })),
    }),
  );
  const operationLimit = $derived(actionLimit(JSON.parse(content) as Draft));
  const visiblePalette = $derived(
    palette.filter((item) =>
      `${item.label}${item.hint}`.includes(search.trim()),
    ),
  );
  const saveStatus = $derived(
    saveError
      ? "保存失败，内容仍在当前页面"
      : saving
        ? "正在保存…"
        : dirty
          ? "等待保存…"
          : `草稿已保存${updatedAt ? ` · ${time(updatedAt, true)}` : ""}`,
  );

  function backupKey(draftId: string) {
    return `adb-workflow-draft:${draftId}`;
  }
  function snapshot(): Draft {
    return { ...JSON.parse(content), revision, updated_at: updatedAt };
  }
  function readBackup(draftId: string): Draft | null {
    try {
      const value = JSON.parse(
        localStorage.getItem(backupKey(draftId)) ?? "null",
      );
      return value &&
        value.id === draftId &&
        Array.isArray(value.nodes) &&
        value.nodes.length <= 200
        ? value
        : null;
    } catch {
      return null;
    }
  }
  function contentOf(draft: Draft) {
    const { revision: _revision, updated_at: _updated, ...value } = draft;
    return canonical(value);
  }
  async function restoreWorkflow(draftId: string) {
    try {
      const workflows = await api<WorkflowDefinition[]>("/workflows");
      if (id !== draftId || savedWorkflow || savingWorkflow) return;
      const latest = workflows.find((item) => item.id === `manual-${draftId}`);
      if (!latest) return;
      savedWorkflow = latest;
      workflowContent = canonical({
        name: latest.name,
        description: latest.description,
        entry: latest.entry,
        nodes: latest.nodes.map((node) => {
          if (node.type === "end")
            return {
              id: node.id,
              kind: "end",
              label: node.label,
              config: { result: node.result },
              targets: {},
            };
          const { id, kind, label, config, targets } = node.step as DraftNode;
          return { id, kind, label, config, targets };
        }),
      });
    } catch (error) {
      if (id === draftId) executionError = message(error);
    }
  }
  function loadDraft(draft: Draft, saved = true) {
    ready = false;
    id = draft.id;
    draftChoice = draft.id;
    revision = draft.revision;
    name = draft.name;
    description = draft.description;
    entry = draft.entry;
    nodes = toCanvas(draft);
    viewport = {
      x: draft.viewport[0],
      y: draft.viewport[1],
      zoom: draft.viewport[2],
    };
    selected = "";
    selectedEdge = "";
    issues = [];
    checksOpen = false;
    updatedAt = draft.updated_at;
    savedWorkflow = null;
    workflowContent = "";
    executionError = "";
    saveError = "";
    savedContent = saved ? content : "";
    history = [content];
    historyIndex = 0;
    const local = readBackup(id);
    recovery =
      saved && local && contentOf(local) !== contentOf(draft) ? local : null;
    window.history.replaceState(
      window.history.state,
      "",
      `/editor?draft=${encodeURIComponent(id)}`,
    );
    try {
      localStorage.setItem("adb-workflow-last-draft", id);
    } catch {
      /* 后台保存仍可使用。 */
    }
    ready = true;
    void restoreWorkflow(draft.id);
  }

  async function boot() {
    ready = false;
    loadingError = "";
    try {
      await session();
      drafts = await api<Draft[]>("/workflow-drafts");
      devices = await api<Device[]>("/devices");
      const explicitDraft = new URLSearchParams(location.search).get("draft");
      let requested = explicitDraft ?? "";
      if (!requested) {
        try {
          requested = localStorage.getItem("adb-workflow-last-draft") ?? "";
        } catch {
          /* 使用后台最近草稿。 */
        }
      }
      const chosen = drafts.find((draft) => draft.id === requested);
      if (explicitDraft && !chosen && !readBackup(requested)) {
        throw new Error("这份草稿已不存在，请返回任务列表查看");
      }
      if (chosen || (!explicitDraft && drafts.length))
        loadDraft(chosen ?? drafts[0]);
      else {
        const local = requested ? readBackup(requested) : null;
        loadDraft(
          local
            ? { ...local, id: crypto.randomUUID(), revision: 0, updated_at: "" }
            : blankDraft(),
          false,
        );
      }
    } catch (error) {
      loadingError = message(error);
    }
  }

  function message(error: unknown) {
    const value =
      error instanceof Error ? error.message : "操作失败，请稍后重试";
    return ["Failed to fetch", "Load failed"].includes(value)
      ? "后台连接中断，请恢复连接后重试保存"
      : value;
  }

  async function persist(): Promise<boolean> {
    if (saving || !ready || recovery) return false;
    if (!name.trim()) {
      saveError = "请填写工作流名称";
      return false;
    }
    const captured = content,
      capturedId = id;
    saving = true;
    saveError = "";
    try {
      const result = await api<Draft>(`/workflow-drafts/${id}`, "PUT", {
        ...JSON.parse(captured),
        revision,
        updated_at: updatedAt,
      });
      if (id !== capturedId) return false;
      revision = result.revision;
      updatedAt = result.updated_at;
      savedContent = captured;
      drafts = [result, ...drafts.filter((draft) => draft.id !== result.id)];
      if (content === captured) {
        try {
          localStorage.removeItem(backupKey(id));
        } catch {
          /* 后台副本已经保存。 */
        }
      }
      return true;
    } catch (error) {
      saveError = message(error);
      return false;
    } finally {
      saving = false;
    }
  }

  async function flush(): Promise<boolean> {
    if (saving || recovery) return false;
    while (dirty) {
      if (!(await persist())) return false;
    }
    return true;
  }

  async function exportDraft() {
    morePopover?.hidePopover();
    if (!(await flush())) {
      notify.error("当前编辑尚未保存，请处理保存问题后再导出");
      return;
    }
    exportRequest = {
      path: `/workflow-drafts/${encodeURIComponent(id)}/export?revision=${revision}`,
      name,
    };
  }

  $effect(() => {
    const value = content;
    if (!ready || recovery || !id || value === savedContent) return;
    try {
      localStorage.setItem(
        backupKey(id),
        JSON.stringify({
          ...JSON.parse(value),
          revision,
          updated_at: updatedAt,
        }),
      );
    } catch {
      /* 后台自动保存继续工作。 */
    }
    if (saving || saveError || !name.trim()) return;
    const timer = setTimeout(() => {
      void persist();
    }, 700);
    return () => clearTimeout(timer);
  });

  async function selectDraft(next: string) {
    if (next === id || switching) return;
    switching = true;
    try {
      if (!(await flush())) return;
      loadDraft(await api<Draft>(`/workflow-drafts/${next}`));
    } catch (error) {
      notify.error(message(error));
    } finally {
      draftChoice = id;
      switching = false;
    }
  }

  async function createDraft() {
    if (switching) return;
    switching = true;
    try {
      if (await flush()) loadDraft(blankDraft(), false);
    } finally {
      switching = false;
    }
  }

  function copyDraft() {
    if (saving || switching) return;
    const copy = snapshot();
    copy.id = crypto.randomUUID();
    copy.revision = 0;
    copy.name = `${name.slice(0, 75)} 副本`;
    copy.updated_at = "";
    recovery = null;
    loadDraft(copy, false);
  }

  async function reloadDraft() {
    switching = true;
    try {
      const server = await api<Draft>(`/workflow-drafts/${id}`);
      try {
        localStorage.removeItem(backupKey(id));
      } catch {
        /* 重新读取后台草稿。 */
      }
      loadDraft(server);
    } catch (error) {
      notify.error(message(error));
    } finally {
      switching = false;
    }
  }

  async function deleteDraft() {
    switching = true;
    try {
      if (revision)
        await api(`/workflow-drafts/${id}?revision=${revision}`, "DELETE");
      try {
        localStorage.removeItem(backupKey(id));
      } catch {
        /* 删除后台草稿不受影响。 */
      }
      drafts = drafts.filter((draft) => draft.id !== id);
      loadDraft(drafts[0] ?? blankDraft(), !!drafts.length);
      deleteDialog?.close();
      notify.success("草稿已删除");
    } catch (error) {
      notify.error(message(error));
    } finally {
      switching = false;
    }
  }

  function clearIssues() {
    issues = [];
    checksOpen = false;
    nodes = nodes.map((node) =>
      node.data.issue
        ? { ...node, data: { ...node.data, issue: undefined } }
        : node,
    );
  }
  async function saveWorkflow(): Promise<WorkflowDefinition | null> {
    savingWorkflow = true;
    executionError = "";
    try {
      if (!(await flush())) return null;
      const submitted = executionContent,
        submittedId = id;
      const result = await api<WorkflowDefinition>(
        "/workflow-drafts/" + id + "/save",
        "POST",
        { revision },
      );
      if (id !== submittedId) return null;
      savedWorkflow = result;
      workflowContent = submitted;
      notify.success("工作流已保存，可直接运行");
      return result;
    } catch (error) {
      executionError = message(error);
      return null;
    } finally {
      savingWorkflow = false;
    }
  }
  function showProgress(nodeId: string) {
    nodes = nodes.map((node) => ({
      ...node,
      data: { ...node.data, running: node.id === nodeId },
    }));
  }
  function remember() {
    clearIssues();
    if (history[historyIndex] === content) return;
    history = [...history.slice(0, historyIndex + 1), content].slice(-80);
    historyIndex = history.length - 1;
  }
  function undo(direction: number) {
    const index = historyIndex + direction;
    if (index < 0 || index >= history.length) return;
    const value: Draft = {
      ...JSON.parse(history[index]),
      revision,
      updated_at: updatedAt,
    };
    name = value.name;
    description = value.description;
    entry = value.entry;
    nodes = toCanvas(value);
    viewport = {
      x: value.viewport[0],
      y: value.viewport[1],
      zoom: value.viewport[2],
    };
    historyIndex = index;
    selected = "";
    selectedEdge = "";
    issues = [];
    checksOpen = false;
  }

  function choose(nodeId: string) {
    selected = nodeId;
    nodes = nodes.map((node) => ({ ...node, selected: node.id === nodeId }));
    selectedEdge = "";
    panel = "settings";
  }
  function addNode(kind: Kind) {
    if (nodes.length > 200) {
      notify.error("最多添加 200 个步骤");
      return;
    }
    const box = canvas?.getBoundingClientRect();
    const point = box
      ? flow.screenToFlowPosition({
          x: box.left + box.width / 2 - 100,
          y: box.top + box.height / 2 - 40,
        })
      : { x: 280, y: 150 };
    const offset = (nodes.length % 4) * 24;
    const node: CanvasNode = {
      id: crypto.randomUUID(),
      type: "step",
      data: {
        kind,
        label: palette.find((item) => item.kind === kind)!.label,
        config: defaults(kind),
        targets: {},
      },
      position: {
        x: Math.round(point.x + offset),
        y: Math.round(point.y + offset),
      },
    };
    nodes = [...nodes, node];
    if (!entry) entry = node.id;
    choose(node.id);
    remember();
  }
  function editNode(patch: Partial<CanvasNode["data"]>) {
    nodes = nodes.map((node) =>
      node.id === selected
        ? { ...node, data: { ...node.data, ...patch } }
        : node,
    );
    remember();
  }
  function setTarget(source: string, port: string, target: string) {
    if (source === "__entry") entry = target;
    else
      nodes = nodes.map((node) => {
        if (node.id !== source) return node;
        const targets = { ...node.data.targets };
        if (target) targets[port] = target;
        else delete targets[port];
        return { ...node, data: { ...node.data, targets } };
      });
    const owners = new Map<string, string[]>();
    for (const loop of nodes.filter((node) => node.data.kind === "loop")) {
      const todo = [loop.data.targets.body],
        seen = new Set<string>();
      while (todo.length) {
        const next = todo.pop();
        if (
          !next ||
          next === loop.id ||
          next === loop.data.targets.done ||
          seen.has(next)
        )
          continue;
        seen.add(next);
        const child = nodes.find((node) => node.id === next);
        if (!child || child.data.kind === "loop") continue;
        if (["break", "continue"].includes(child.data.kind)) {
          owners.set(next, [...(owners.get(next) ?? []), loop.id]);
          continue;
        }
        todo.push(...Object.values(child.data.targets));
      }
    }
    nodes = nodes.map((node) =>
      owners.get(node.id)?.length === 1
        ? {
            ...node,
            data: {
              ...node.data,
              config: { ...node.data.config, loop: owners.get(node.id)![0] },
            },
          }
        : node,
    );
    remember();
  }
  function removeNode() {
    if (!selected || selected === "__entry") return;
    const removed = selected;
    nodes = nodes
      .filter((node) => node.id !== removed)
      .map((node) => ({
        ...node,
        data: {
          ...node.data,
          targets: Object.fromEntries(
            Object.entries(node.data.targets).filter(
              ([, target]) => target !== removed,
            ),
          ),
          config:
            node.data.config.loop === removed
              ? { ...node.data.config, loop: "" }
              : node.data.config,
        },
      }));
    if (entry === removed) entry = "";
    selected = "";
    remember();
  }
  function removeEdge() {
    const edge = edges.find((item) => item.id === selectedEdge);
    if (!edge || !edge.deletable) return;
    setTarget(edge.source, edge.sourceHandle ?? "next", "");
    selectedEdge = "";
  }
  function check() {
    issues = checkGraph(snapshot());
    checksOpen = true;
    nodes = nodes.map((node) => ({
      ...node,
      data: {
        ...node.data,
        issue: issues.find((issue) => issue.node === node.id)?.message,
      },
    }));
  }
  async function arrange() {
    const graph = new graphlib.Graph()
      .setGraph({ rankdir: "TB", nodesep: 80, ranksep: 90 })
      .setDefaultEdgeLabel(() => ({}));
    for (const node of nodes)
      graph.setNode(node.id, {
        width: node.id === "__entry" ? 150 : 230,
        height: node.id === "__entry" ? 80 : 130,
      });
    for (const edge of edges) graph.setEdge(edge.source, edge.target);
    layout(graph);
    const start = graph.node("__entry");
    const offset = { x: 280 - (start.x - 75), y: -(start.y - 40) };
    nodes = nodes.map((node) => {
      const point = graph.node(node.id);
      return {
        ...node,
        position: {
          x: point.x - (node.id === "__entry" ? 75 : 115) + offset.x,
          y: point.y - (node.id === "__entry" ? 40 : 65) + offset.y,
        },
      };
    });
    remember();
    await tick();
    void flow.fitView({ padding: 0.18, duration: 200 });
  }
  function moveNode(x: number, y: number) {
    nodes = nodes.map((node) =>
      node.id === selected
        ? {
            ...node,
            position: { x: node.position.x + x, y: node.position.y + y },
          }
        : node,
    );
    remember();
  }
  function restoreBackup() {
    if (!recovery) return;
    const value = recovery;
    const serverRevision = revision;
    recovery = null;
    loadDraft({ ...value, revision: serverRevision }, false);
    recovery = null;
  }
  function discardBackup() {
    try {
      localStorage.removeItem(backupKey(id));
    } catch {
      /* 忽略本地恢复内容。 */
    }
    recovery = null;
  }
  function toggleTheme() {
    dark = !dark;
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    try {
      localStorage.setItem("adb-studio-theme", dark ? "dark" : "light");
    } catch {
      /* 当前页面仍可切换。 */
    }
  }
  function onKey(event: KeyboardEvent) {
    const target = event.target as HTMLElement;
    if (
      target.closest(
        "input, textarea, select, [contenteditable=true], dialog, [popover]",
      )
    )
      return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z") {
      event.preventDefault();
      undo(event.shiftKey ? 1 : -1);
    }
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      if (selectedEdge) removeEdge();
      else removeNode();
    }
  }
  onMount(() => {
    dark = document.documentElement.dataset.theme === "dark";
    void boot();
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (ready && dirty) event.preventDefault();
    };
    window.addEventListener("beforeunload", beforeUnload);
    const deviceTimer = setInterval(() => {
      if (ready)
        void api<Device[]>("/devices")
          .then((items) => (devices = items))
          .catch(() => {});
    }, 4000);
    return () => {
      clearInterval(deviceTimer);
      window.removeEventListener("beforeunload", beforeUnload);
    };
  });
</script>

<svelte:window onkeydown={onKey} />

<main class="workflow-editor" class:dark>
  <header class="editor-header" aria-label="工作流顶栏">
    <a
      href="/"
      data-sveltekit-reload
      class="editor-back"
      title="返回工作台"
      aria-label="返回工作台"
      ><ArrowLeft size={19} /><span class="header-back-label">返回工作台</span
      ></a
    >
    <Workflow size={21} class="editor-mark" />
    <h1 class="visually-hidden">编排工作流</h1>
    <label class="header-name"
      ><span class="visually-hidden">工作流名称</span><input
        aria-label="工作流名称"
        title={name}
        placeholder="工作流名称"
        maxlength="80"
        bind:value={name}
        onchange={remember}
        disabled={!ready || switching}
      /></label
    >
    <div class="header-actions">
      {#if hasProblems}<button
          class="header-problems"
          title="查看待处理问题"
          aria-label="查看待处理问题"
          aria-haspopup="dialog"
          aria-expanded={problemsOpen}
          popovertarget="editor-problems"
          ><AlertCircle size={17} /><span class="status-label">需要处理</span
          ></button
        >
      {:else}<span
          class="save-status"
          role="status"
          title={ready ? saveStatus : "正在读取…"}
          aria-label={ready ? saveStatus : "正在读取…"}
          ><span class="status-label"
            >{ready ? saveStatus : "正在读取…"}</span
          ></span
        >{/if}
      <button
        class="editor-button wide-action"
        disabled={!ready || saving || switching || !!recovery}
        onclick={check}><Check size={16} />检查草稿</button
      >
      <button
        class="editor-button primary"
        title="保存工作流"
        aria-label="保存工作流"
        disabled={!ready || saving || switching || savingWorkflow || !!recovery}
        onclick={saveWorkflow}
        ><Save size={16} /><span class="save-label"
          >{savingWorkflow ? "正在保存…" : "保存"}</span
        ></button
      >
      <EditorRunPanel
        workflowId={id ? `manual-${id}` : ""}
        {savedWorkflow}
        {devices}
        bind:device={deviceId}
        changed={workflowContent !== executionContent}
        onstate={showProgress}
        disabled={!ready || switching || !!recovery}
        {savingWorkflow}
        onsave={saveWorkflow}
      />
      <button
        class="editor-button more-trigger"
        title="草稿与更多操作"
        aria-label="草稿与更多操作"
        aria-haspopup="dialog"
        aria-expanded={moreOpen}
        popovertarget="editor-more"
        ><MoreHorizontal size={20} /><span class="more-label">更多</span
        ></button
      >
    </div>
    <div
      id="editor-more"
      class="editor-popover"
      bind:this={morePopover}
      popover="auto"
      role="dialog"
      tabindex="-1"
      aria-label="草稿与更多操作"
      ontoggle={(event) => (moreOpen = event.newState === "open")}
    >
      <div class="popover-heading">
        <h2>草稿与设置</h2>
        <button
          class="popover-close"
          aria-label="关闭更多操作"
          onclick={() => morePopover?.hidePopover()}><X size={18} /></button
        >
      </div>
      <label class="draft-picker"
        >打开草稿<Dropdown
          label="打开草稿"
          bind:value={draftChoice}
          disabled={!ready || saving || switching || !!recovery}
          items={[
            { value: id, label: name || "未命名工作流" },
            ...drafts
              .filter((draft) => draft.id !== id)
              .map((draft) => ({ value: draft.id, label: draft.name })),
          ]}
          onchange={(next) => void selectDraft(next)}
        /></label
      >
      <div class="popover-actions">
        <button
          class="editor-button"
          onclick={createDraft}
          disabled={!ready || saving || switching || !!recovery}
          ><Plus size={16} />新建</button
        ><button
          class="editor-button"
          onclick={copyDraft}
          disabled={!ready || saving || switching || !!recovery}
          ><Copy size={16} />保存副本</button
        ><button
          class="editor-button danger"
          onclick={() => {
            morePopover?.hidePopover();
            deleteDialog?.showModal();
          }}
          disabled={!ready || saving || switching || !!recovery}
          ><Trash2 size={16} />删除草稿</button
        >
      </div>
      <div class="popover-actions compact-actions">
        <button
          class="editor-button"
          disabled={!ready || saving || switching || !!recovery}
          onclick={() => {
            morePopover?.hidePopover();
            check();
          }}><Check size={16} />检查草稿</button
        ><button
          class="editor-button"
          disabled={!ready ||
            saving ||
            switching ||
            savingWorkflow ||
            !!recovery}
          onclick={() => {
            morePopover?.hidePopover();
            void saveWorkflow();
          }}>保存工作流</button
        >
      </div>
      <div class="popover-actions theme-action">
        <button
          class="editor-button"
          disabled={!ready ||
            saving ||
            switching ||
            !!recovery ||
            !!exportRequest}
          onclick={exportDraft}
          ><Download size={16} aria-hidden="true" />导出当前草稿</button
        >
        <button class="editor-button" onclick={toggleTheme}
          >{#if dark}<Sun size={16} />切换日间模式{:else}<Moon
              size={16}
            />切换夜间模式{/if}</button
        >
      </div>
    </div>
    <div
      id="editor-problems"
      class="editor-popover"
      bind:this={problemsPopover}
      popover="auto"
      role="dialog"
      tabindex="-1"
      aria-label="待处理问题"
      ontoggle={(event) => (problemsOpen = event.newState === "open")}
    >
      <div class="popover-heading">
        <h2>需要处理</h2>
        <button
          class="popover-close"
          aria-label="关闭问题详情"
          onclick={() => problemsPopover?.hidePopover()}><X size={18} /></button
        >
      </div>
      {#if executionError}<div class="problem-block" role="alert">
          <p>{executionError}</p>
        </div>{/if}
      {#if saveError}<div class="problem-block" role="alert">
          <p>{saveError}</p>
          <div class="popover-actions">
            <button
              class="editor-button"
              onclick={() => {
                void persist();
              }}
              disabled={saving}>重试保存</button
            ><button class="editor-button" onclick={copyDraft} disabled={saving}
              >保存为副本</button
            ><button
              class="editor-button"
              onclick={reloadDraft}
              disabled={saving}>重新读取后台内容</button
            >
          </div>
        </div>{/if}
      {#if recovery}<div class="problem-block" role="alert">
          <p>发现此浏览器上次未保存的内容。</p>
          <div class="popover-actions">
            <button class="editor-button" onclick={restoreBackup}
              >恢复内容</button
            ><button class="editor-button" onclick={discardBackup}
              >使用后台草稿</button
            >
          </div>
        </div>{/if}
      {#if !hasProblems}<p class="popover-hint">目前没有待处理的问题。</p>{/if}
    </div>
  </header>
  {#if loadingError}<div class="editor-loading">
      <AlertCircle size={24} />
      <h2>无法读取草稿</h2>
      <p>{loadingError}</p>
      <button class="editor-button" onclick={boot}
        ><RefreshCw size={16} />重试</button
      >
    </div>
  {:else if !ready}<div class="editor-loading">正在读取工作流草稿…</div>
  {:else}
    <nav class="compact-panels" aria-label="编辑面板">
      <button
        class:active={panel === "palette"}
        onclick={() => (panel = "palette")}>添加步骤</button
      ><button
        class:active={panel === "settings"}
        onclick={() => (panel = "settings")}>步骤设置</button
      >
    </nav>
    <div class="editor-body" inert={switching}>
      <aside class="editor-palette" class:compact-hidden={panel !== "palette"}>
        <div class="side-heading">
          <h2>添加步骤</h2>
          <p>点击添加，也可拖动画布上的步骤</p>
        </div>
        <label class="palette-search"
          ><span class="visually-hidden">搜索步骤</span><input
            type="search"
            placeholder="搜索步骤"
            bind:value={search}
          /></label
        >
        {#each ["基础动作", "流程控制"] as group}
          {#if visiblePalette.some((item) => item.group === group)}<h3
              class="palette-group"
            >
              {group}
            </h3>{/if}
          {#each visiblePalette.filter((item) => item.group === group) as item (item.kind)}
            <button class="palette-item" onclick={() => addNode(item.kind)}
              ><span
                ><strong>{item.label}</strong><small>{item.hint}</small></span
              ><Plus size={16} /></button
            >
          {/each}
        {/each}
        {#if !visiblePalette.length}<p class="palette-empty">
            没有符合的步骤
          </p>{/if}
      </aside>

      <section class="canvas-section" aria-label="工作流画布">
        <div class="canvas-toolbar">
          <div class="toolbar-group">
            <button
              aria-label="撤销"
              title="撤销"
              disabled={historyIndex <= 0}
              onclick={() => undo(-1)}><Undo2 size={17} /></button
            ><button
              aria-label="重做"
              title="重做"
              disabled={historyIndex >= history.length - 1}
              onclick={() => undo(1)}><Redo2 size={17} /></button
            ><span>{nodes.length - 1} 个步骤</span>
          </div>
          <div class="toolbar-group">
            <button onclick={arrange}>整理位置</button><button
              onclick={() => flow.fitView({ padding: 0.2, duration: 200 })}
              ><Scan size={16} />查看全图</button
            >
          </div>
        </div>
        <div class="workflow-canvas" bind:this={canvas}>
          <SvelteFlow
            bind:nodes
            {edges}
            bind:viewport
            {nodeTypes}
            minZoom={0.15}
            maxZoom={2}
            deleteKey={null}
            colorMode={dark ? "dark" : "light"}
            isValidConnection={(connection) =>
              connection.source !== connection.target &&
              connection.target !== "__entry" &&
              connection.sourceHandle !== "control"}
            onconnect={(connection) =>
              setTarget(
                connection.source,
                connection.sourceHandle ?? "next",
                connection.target,
              )}
            onnodeclick={({ node }) => choose(node.id)}
            onedgeclick={({ edge }) => {
              selectedEdge = edge.id;
              selected = "";
              nodes = nodes.map((node) => ({ ...node, selected: false }));
              panel = "settings";
            }}
            onpaneclick={() => {
              selected = "";
              selectedEdge = "";
            }}
            onnodedragstop={remember}
            ariaLabelConfig={{
              "controls.zoomIn.ariaLabel": "放大画布",
              "controls.zoomOut.ariaLabel": "缩小画布",
              "controls.fitView.ariaLabel": "查看全图",
              "controls.interactive.ariaLabel": "切换画布操作",
            }}
          >
            <Background gap={24} patternColor="var(--line)" /><Controls
              showLock={false}
            />
          </SvelteFlow>
        </div>
        {#if checksOpen}<div class="graph-checks" aria-live="polite">
            <div>
              <h3>
                {issues.length
                  ? `有 ${issues.length} 处待补齐`
                  : "草稿连接与基本参数已检查"}
              </h3>
              <button onclick={() => (checksOpen = false)}>收起</button>
            </div>
            {#if issues.length}<ul>
                {#each issues as issue}<li>
                    <button
                      onclick={() => {
                        if (issue.node) choose(issue.node);
                      }}>{issue.message}</button
                    >
                  </li>{/each}
              </ul>{:else}<p>可保存并运行。</p>{/if}
          </div>{/if}
      </section>

      <aside
        class="editor-settings"
        class:compact-hidden={panel !== "settings"}
      >
        <div class="side-heading">
          <h2>
            {current ? "步骤设置" : selectedEdge ? "连线设置" : "工作流设置"}
          </h2>
        </div>
        <div class="settings-content">
          <label
            >选择步骤<Dropdown
              label="选择步骤"
              value={selected}
              items={[
                { value: "", label: "工作流设置" },
                ...nodes.map((node) => ({
                  value: node.id,
                  label: node.data.label,
                })),
              ]}
              onchange={choose}
            /></label
          >
          {#if current}
            <StepInspector
              {operationLimit}
              deviceNames={Object.fromEntries(
                devices.map((device) => [device.id, device.name]),
              )}
              node={current}
              {nodes}
              {entry}
              onLabel={(value) => editNode({ label: value })}
              onConfig={(config: Config) => editNode({ config })}
              onTarget={(port, target) => setTarget(current.id, port, target)}
              onEntry={() => {
                entry = current.id;
                remember();
              }}
              onRemove={removeNode}
              onMove={moveNode}
              onMark={() => {
                markCondition = null;
                markNode = current;
              }}
              onMarkCondition={(index) => {
                markCondition = index;
                markNode = current;
              }}
            />
          {:else if selectedEdge}
            <p class="settings-hint">
              {edges.find((edge) => edge.id === selectedEdge)?.label}
            </p>
            {#if edges.find((edge) => edge.id === selectedEdge)?.deletable}<button
                class="editor-button danger"
                onclick={removeEdge}><Trash2 size={16} />删除这条连线</button
              >{:else}<p class="settings-hint">
                此连线由所属循环决定，可在步骤设置中修改所属循环。
              </p>{/if}
          {:else}
            <label
              >工作流说明<textarea
                rows="4"
                maxlength="2000"
                bind:value={description}
                onchange={remember}></textarea></label
            >
            <label
              >开始执行的步骤<Dropdown
                label="开始执行的步骤"
                bind:value={entry}
                items={[
                  { value: "", label: "请选择入口" },
                  ...nodes
                    .filter((node) => node.id !== "__entry")
                    .map((node) => ({
                      value: node.id,
                      label: node.data.label,
                    })),
                ]}
                placeholder="请选择入口"
                onchange={remember}
              /></label
            >
            <p class="settings-hint">
              {operationLimit === null
                ? "连接完整后显示手机操作次数上限。"
                : `手机操作上限：${operationLimit} 次（含按键、点击、滑动；分支不一定全部执行）。`}
            </p>
            <p class="settings-hint">
              选中画布上的步骤，可修改参数和连接去向。每个出口只能连接一个下一步。
            </p>
            <p class="settings-hint">
              添加与删除支持撤销；修改会自动保存为草稿。
            </p>
          {/if}
        </div>
      </aside>
    </div>
  {/if}
</main>
<WorkflowExport
  request={exportRequest}
  onclose={() => (exportRequest = null)}
/>
{#if markNode}<CoordinatePicker
    node={markNode}
    conditionIndex={markCondition}
    {devices}
    initialDevice={deviceId}
    onClose={() => (markNode = null)}
    onSave={(profile) => {
      const markedId = markNode?.id;
      const mergeProfile = (items: (typeof profile)[] = []) => [
        ...items.filter(
          (item) =>
            !(
              item.device_id === profile.device_id &&
              item.width === profile.width &&
              item.height === profile.height &&
              item.display === profile.display
            ),
        ),
        profile,
      ];
      nodes = nodes.map((node) => {
        if (node.id !== markedId) return node;
        const config =
          markCondition === null
            ? {
                ...node.data.config,
                profiles: mergeProfile(node.data.config.profiles),
              }
            : {
                ...node.data.config,
                conditions: node.data.config.conditions?.map(
                  (condition, index) =>
                    index === markCondition
                      ? {
                          ...condition,
                          regions: mergeProfile(condition.regions),
                        }
                      : condition,
                ),
              };
        return { ...node, data: { ...node.data, config } };
      });
      deviceId = profile.device_id;
      markNode = null;
      remember();
    }}
  />{/if}

<dialog bind:this={deleteDialog}>
  <h2>删除这份草稿？</h2>
  <p>
    “{name}”的草稿删除后无法恢复。任务内容仍保留，可从任务卡片继续修改，但原画布布局无法恢复。
  </p>
  <div class="dialog-actions">
    <button
      class="editor-button"
      disabled={switching}
      onclick={() => deleteDialog?.close()}>取消</button
    ><button
      class="editor-button danger"
      disabled={switching}
      onclick={deleteDraft}>删除草稿</button
    >
  </div>
</dialog>

<style>
  .workflow-editor {
    height: 100dvh;
    min-height: 580px;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    color: var(--text);
  }
  .editor-header {
    display: flex;
    align-items: center;
    flex: 0 0 64px;
    height: 64px;
    min-height: 64px;
    max-height: 64px;
    box-sizing: border-box;
    flex-wrap: nowrap;
    gap: 12px;
    padding: 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .editor-back {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    gap: 7px;
    min-height: 40px;
    font-size: 13px;
    color: var(--dim);
    text-decoration: none;
    white-space: nowrap;
  }
  .editor-header :global(.editor-mark) {
    flex-shrink: 0;
    color: var(--accent);
  }
  .header-name {
    flex: 1;
    min-width: 0;
    margin: 0;
  }
  .header-name input {
    width: 100%;
    min-width: 0;
    height: 40px;
    min-height: 40px;
    padding: 8px;
    border-color: transparent;
    background: transparent;
    color: var(--text);
    font-size: 16px;
    font-weight: 600;
    text-overflow: ellipsis;
  }
  .header-name input:hover {
    border-color: var(--line);
  }
  .header-name input:focus {
    border-color: var(--accent);
    background: var(--panel2);
  }
  .header-actions {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    flex-wrap: nowrap;
    gap: 8px;
  }
  .editor-button {
    display: inline-flex;
    justify-content: center;
    align-items: center;
    gap: 7px;
    min-height: 38px;
    padding: 8px 12px;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    font-size: 13px;
    white-space: nowrap;
  }
  .editor-button.primary {
    background: var(--accent);
    color: var(--primary-ink);
    border-color: var(--accent);
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .danger {
    color: var(--red);
  }
  .save-status,
  .header-problems {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--dim);
    white-space: nowrap;
  }
  .header-problems {
    min-height: 38px;
    border: 0;
    padding: 8px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--red) 10%, var(--panel));
    color: var(--red);
  }
  .editor-popover {
    position: fixed;
    inset: 70px 16px auto auto;
    margin: 0;
    width: min(380px, calc(100vw - 32px));
    max-height: calc(100dvh - 86px);
    overflow: auto;
    scrollbar-gutter: stable;
    padding: 18px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
    white-space: normal;
  }
  .popover-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }
  .popover-heading h2 {
    font-size: 16px;
  }
  .popover-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--dim);
  }
  .draft-picker {
    width: 100%;
    margin: 0 0 14px;
    font-size: 13px;
  }
  .popover-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .compact-actions {
    display: none;
    margin-top: 14px;
  }
  .theme-action {
    margin-top: 16px;
  }
  .problem-block + .problem-block {
    margin-top: 16px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
  .problem-block p {
    color: var(--red);
    font-size: 13px;
    line-height: 1.7;
    overflow-wrap: anywhere;
  }
  .problem-block .popover-actions {
    margin-top: 12px;
  }
  .popover-hint {
    color: var(--dim);
    font-size: 13px;
  }
  .editor-body {
    display: grid;
    grid-template-columns: 215px minmax(0, 1fr) 290px;
    flex: 1;
    min-height: 0;
  }
  .editor-palette,
  .editor-settings {
    background: var(--panel);
    overflow-y: auto;
    scrollbar-gutter: stable;
  }
  .editor-palette {
    border-right: 1px solid var(--line);
  }
  .editor-settings {
    border-left: 1px solid var(--line);
  }
  .side-heading {
    padding: 19px 18px 12px;
  }
  .side-heading h2 {
    font-size: 16px;
  }
  .side-heading p {
    font-size: 12px;
    line-height: 1.6;
    color: var(--dim);
    margin-top: 7px;
  }
  .palette-search {
    margin: 0 14px 15px;
  }
  .palette-search input {
    min-height: 38px;
    font-size: 13px;
  }
  .palette-group {
    color: var(--dim);
    font-size: 12px;
    font-weight: 500;
    padding: 8px 18px;
  }
  .palette-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: calc(100% - 24px);
    text-align: left;
    margin: 0 12px 7px;
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 11px 12px;
    background: var(--panel);
    color: var(--text);
  }
  .palette-item:hover {
    border-color: var(--accent);
    background: var(--accentsoft);
  }
  .palette-item strong {
    display: block;
    font-size: 13px;
    font-weight: 600;
  }
  .palette-item small {
    display: block;
    color: var(--dim);
    font-size: 11px;
    line-height: 1.6;
    margin-top: 3px;
  }
  .palette-empty {
    padding: 18px;
    color: var(--dim);
  }
  .canvas-section {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .canvas-toolbar {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 9px 14px;
    border-bottom: 1px solid var(--line);
    background: var(--panel2);
  }
  .toolbar-group {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .toolbar-group button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 36px;
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--panel);
    color: var(--text);
    font-size: 12px;
  }
  .toolbar-group span {
    font-size: 12px;
    color: var(--dim);
  }
  .workflow-canvas {
    flex: 1;
    min-height: 280px;
    --xy-background-color: var(--bg);
    --xy-edge-stroke: var(--dim);
    --xy-edge-stroke-selected: var(--accent);
    --xy-edge-stroke-width: 1.7;
    --xy-edge-label-background-color: var(--panel);
    --xy-edge-label-color: var(--text);
    --xy-controls-button-background-color: var(--panel);
    --xy-controls-button-color: var(--text);
    --xy-controls-button-border-color: var(--line);
  }
  :global(.workflow-canvas .svelte-flow__attribution) {
    font-size: 10px;
    background: var(--panel);
    color: var(--dim);
  }
  .settings-content {
    padding: 0 18px 24px;
  }
  .settings-content label {
    font-size: 13px;
  }
  textarea {
    width: 100%;
    resize: vertical;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel2);
    color: var(--text);
    padding: 10px;
    font: inherit;
    line-height: 1.7;
  }
  .settings-hint {
    color: var(--dim);
    font-size: 13px;
    line-height: 1.8;
    margin: 18px 0;
  }
  .graph-checks {
    max-height: 210px;
    overflow: auto;
    scrollbar-gutter: stable;
    border-top: 1px solid var(--line);
    background: var(--panel);
    padding: 12px 18px;
  }
  .graph-checks > div {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .graph-checks h3 {
    font-size: 13px;
  }
  .graph-checks button {
    background: none;
    color: var(--dim);
    border: 0;
    font-size: 12px;
    text-align: left;
    line-height: 1.7;
  }
  .graph-checks ul {
    padding-left: 18px;
    color: var(--red);
  }
  .graph-checks li button {
    color: var(--red);
  }
  .graph-checks p {
    color: var(--dim);
    font-size: 12px;
    margin-top: 8px;
  }
  .editor-loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    flex: 1;
    padding: 30px;
  }
  .editor-loading h2 {
    font-size: 20px;
  }
  .compact-panels {
    display: none;
  }
  dialog {
    max-width: 420px;
    width: calc(100% - 32px);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 26px;
    background: var(--panel);
    color: var(--text);
    margin: auto;
  }
  dialog h2 {
    font-size: 20px;
  }
  dialog p {
    line-height: 1.7;
    margin-top: 12px;
  }
  .dialog-actions {
    display: flex;
    justify-content: end;
    gap: 10px;
    margin-top: 25px;
  }
  @media (max-width: 1280px) {
    .header-back-label,
    .save-status,
    .status-label {
      display: none;
    }
  }
  @media (max-width: 1100px) {
    .editor-body {
      grid-template-columns: 185px minmax(0, 1fr) 260px;
    }
    .wide-action {
      display: none;
    }
    .compact-actions {
      display: flex;
    }
  }
  @media (max-width: 820px) {
    .workflow-editor {
      min-height: 740px;
      height: auto;
      padding-bottom: 70px;
    }
    .editor-header {
      padding-inline: 12px;
      gap: 8px;
    }
    .compact-panels {
      display: flex;
      position: fixed;
      inset: auto auto 16px 50%;
      transform: translateX(-50%);
      z-index: 20;
      gap: 8px;
      padding: 6px;
      border: 1px solid var(--line);
      border-radius: 12px;
      background: var(--panel);
      box-shadow: 0 4px 20px rgb(0 0 0 / 15%);
    }
    .compact-panels button {
      min-height: 40px;
      border: 1px solid transparent;
      border-radius: 7px;
      color: var(--dim);
      background: var(--panel);
      padding: 9px 14px;
      font-size: 13px;
      white-space: nowrap;
    }
    .compact-panels button.active {
      background: var(--accentsoft);
      color: var(--accent2);
      border-color: var(--accent);
    }
    .editor-body {
      display: flex;
      flex-direction: column;
    }
    .canvas-section {
      order: 0;
      height: 460px;
      flex-shrink: 0;
    }
    .editor-palette,
    .editor-settings {
      order: 1;
      max-height: 420px;
      border: 0;
      border-top: 1px solid var(--line);
    }
    .compact-hidden {
      display: none;
    }
    .toolbar-group {
      gap: 4px;
    }
    .toolbar-group span {
      display: none;
    }
    .palette-item {
      min-height: 52px;
    }
  }
  @media (max-width: 600px) {
    .editor-header {
      padding-inline: 8px;
      gap: 4px;
    }
    .editor-header :global(.editor-mark),
    .save-label,
    .more-label,
    .save-status {
      display: none;
    }
    .header-name input {
      padding-inline: 4px;
      font-size: 14px;
    }
    .header-actions {
      gap: 6px;
    }
    .editor-back {
      min-width: 32px;
      justify-content: center;
    }
    .header-actions .editor-button {
      min-width: 38px;
      padding-inline: 8px;
    }
  }
</style>
