<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    Smartphone,
    Workflow as FlowIcon,
    CalendarClock,
    History,
    Settings as SettingsIcon,
    Play,
    Square,
    ArrowLeft,
    ArrowDown,
    Download,
    Check,
    Plus,
    LoaderCircle,
    Trash2,
    Sun,
    Moon,
  } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import DeviceManager from "$lib/components/DeviceManager.svelte";
  import TaskList from "$lib/components/TaskList.svelte";
  import Badge from "$lib/components/ui/badge/badge.svelte";
  import {
    api,
    session,
    photo,
    busy,
    time,
    state as runState,
    type Device,
    type TapPoint,
    type Workflow,
    type Run,
    type Schedule,
    type Settings,
    type Event,
    type Rule,
  } from "$lib/api";

  const tabs = [
    { id: "devices", label: "设备管理", icon: Smartphone },
    { id: "workflows", label: "自动化任务", icon: FlowIcon },
    { id: "schedules", label: "定时任务", icon: CalendarClock },
    { id: "runs", label: "执行记录", icon: History },
    { id: "settings", label: "系统设置", icon: SettingsIcon },
  ];
  let tab = $state("devices");
  let selectedDeviceId = $state("");
  let workflowOpen = $state(false);
  let contentElement = $state<HTMLElement>();
  $effect(() => {
    const currentTab = tab;
    const currentDeviceId = selectedDeviceId;
    const currentWorkflowOpen = workflowOpen;
    void tick().then(() => {
      if (
        tab === currentTab &&
        selectedDeviceId === currentDeviceId &&
        workflowOpen === currentWorkflowOpen
      )
        contentElement?.scrollTo(0, 0);
    });
  });
  let theme = $state("light");
  let clock = $state("");
  function toggleTheme() {
    theme = theme === "light" ? "dark" : "light";
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem("adb-studio-theme", theme);
    } catch {
      /* 当前页面仍可切换主题 */
    }
  }
  let devices = $state<Device[]>([]);
  let workflows = $state<Workflow[]>([]);
  let runs = $state<Run[]>([]);
  let schedules = $state<Schedule[]>([]);
  let settings = $state<Settings | null>(null);
  let connected = $state(false);
  let loading = $state(true);
  let working = $state("");
  let error = $state("");
  let notice = $state("");
  let lastUpdated = $state<string | null>(null);
  let renameDialog = $state<HTMLDialogElement>();
  let editDevice = $state<Device | null>(null);
  let rename = $state("");
  let pictures = $state<Record<string, string>>({});
  let selectedWorkflow = $state("swipe");
  let selectedVersion = $state(0);
  let targetDevice = $state("3430131496001EB");
  let pointDialog = $state<HTMLDialogElement>();
  let pointDevice = $state<Device | null>(null);
  let pointImage = $state("");
  let pendingPoint = $state<TapPoint | null>(null);
  let pointWidth = $state(0);
  let pointHeight = $state(0);
  let tapCount = $state(10);
  let tapInterval = $state(5);
  let scheduleTapCount = $state(10);
  let scheduleTapInterval = $state(5);
  let swipeMinutes = $state(15);
  let swipeInterval = $state(25);
  let scheduleSwipeMinutes = $state(15);
  let scheduleSwipeInterval = $state(25);
  let selectedRun = $state("");
  let events = $state<Event[]>([]);
  let clearDialog = $state<HTMLDialogElement>();
  let runList = $state<HTMLDivElement>();
  let emptyRunButton = $state<HTMLElement | null>(null);
  let clearRequest = $state<{ ids: string[]; description: string } | null>(
    null,
  );
  let editingSchedule = $state("");
  let scheduleEnabled = $state(true);
  let scheduleName = $state("");
  let scheduleDevice = $state("3430131496001EB");
  let scheduleWorkflow = $state("swipe");
  let ruleKind = $state("daily");
  let ruleTime = $state("09:00");
  let ruleDate = $state("");
  let ruleMinutes = $state(60);
  let ruleDays = $state<number[]>([1, 2, 3, 4, 5]);
  let model = $state({
    endpoint: "",
    model: "",
    api: "responses",
    key_env: "MODEL_KEY",
  });
  const activeRuns = $derived(runs.filter(busy));
  const finishedRuns = $derived(runs.filter((r) => !busy(r)));
  const activeWorkflows = $derived(workflows.filter((w) => w.active));
  const flow = $derived(
    workflows.find(
      (w) =>
        w.id === selectedWorkflow &&
        (selectedVersion === 0 || w.version === selectedVersion),
    ),
  );
  const launchDevice = $derived(
    devices.find((device) => device.id === targetDevice),
  );
  const launchRun = $derived(
    activeRuns.find((run) => run.device_id === targetDevice),
  );
  const launchBlocked = $derived(
    !launchDevice?.online
      ? "手机离线，连接后可开始。"
      : launchDevice.locked
        ? "请先在设备详情中唤醒并解锁手机。"
        : launchRun
          ? `当前任务：${runState(launchRun.state)}。停止后可开始新任务。`
          : flow?.id === "point-click" && !launchDevice.tap_point
            ? "请先标注这台手机的点击位置。"
            : "",
  );
  $effect(() => {
    if (flow && flow.devices.length && !flow.devices.includes(targetDevice))
      targetDevice = flow.devices[0];
  });
  const run = $derived(runs.find((r) => r.id === selectedRun));
  $effect(() => {
    if (editDevice) {
      if (renameDialog && !renameDialog.open) renameDialog.showModal();
    } else renameDialog?.close();
  });
  let reloadBusy = false;
  let dirty = false;
  let alive = true;
  async function load() {
    if (reloadBusy) {
      dirty = true;
      return;
    }
    reloadBusy = true;
    try {
      do {
        dirty = false;
        const [d, w, r, s, config] = await Promise.all([
          api<Device[]>("/devices"),
          api<Workflow[]>("/workflows"),
          api<Run[]>("/runs"),
          api<Schedule[]>("/schedules"),
          api<Settings>("/settings"),
        ]);
        if (!alive) return;
        devices = d;
        workflows = w.sort((a, b) => b.version - a.version);
        runs = r;
        reconcileRunSelection();
        schedules = s;
        settings = config;
        connected = true;
        lastUpdated = new Date().toISOString();
        if (!model.endpoint) model = { ...config.model };
        if (selectedRun) await loadRunEvents(selectedRun);
      } while (dirty && alive);
    } catch (e) {
      connected = false;
      error = e instanceof Error ? e.message : "连接失败";
    } finally {
      loading = false;
      reloadBusy = false;
    }
  }
  async function action(
    key: string,
    fn: () => Promise<unknown>,
    message = "已保存",
  ) {
    if (working) return;
    working = key;
    error = "";
    notice = "";
    try {
      await fn();
      notice = message;
      await load();
    } catch (e) {
      error = e instanceof Error ? e.message : "操作失败";
    } finally {
      working = "";
    }
  }
  async function capture(d: Device) {
    await action(
      `photo-${d.id}`,
      async () => {
        const url = await photo(d.id);
        if (pictures[d.id]) URL.revokeObjectURL(pictures[d.id]);
        pictures = { ...pictures, [d.id]: url };
      },
      "截图已更新",
    );
  }
  async function controlScreen(d: Device, operation: "wake" | "lock") {
    let message = "";
    await action(
      `screen-${d.id}`,
      async () => {
        const result = await api<{ message: string }>(
          `/devices/${d.id}/screen`,
          "POST",
          { action: operation },
        );
        message = result.message;
      },
      "",
    );
    if (message) notice = message;
  }
  async function markPoint(d: Device) {
    await action(
      "mark-point",
      async () => {
        const url = await photo(d.id);
        if (pointImage) URL.revokeObjectURL(pointImage);
        pointImage = url;
        pointDevice = d;
        pendingPoint = null;
        pointWidth = 0;
        pointHeight = 0;
        pointDialog?.showModal();
      },
      "请在截图上点选位置",
    );
  }
  function choosePoint(e: MouseEvent) {
    const box = e.currentTarget as HTMLButtonElement;
    const rect = box.getBoundingClientRect();
    if (!pointWidth || !pointHeight) return;
    pendingPoint = {
      x: Math.min(
        pointWidth - 1,
        Math.max(
          0,
          Math.floor(((e.clientX - rect.left) / rect.width) * pointWidth),
        ),
      ),
      y: Math.min(
        pointHeight - 1,
        Math.max(
          0,
          Math.floor(((e.clientY - rect.top) / rect.height) * pointHeight),
        ),
      ),
      width: pointWidth,
      height: pointHeight,
    };
  }
  async function confirmPoint() {
    if (!pointDevice || !pendingPoint) return;
    const deviceId = pointDevice.id;
    const point = pendingPoint;
    await action(
      "save-point",
      async () => {
        await api(`/devices/${deviceId}`, "PUT", { tap_point: point });
        targetDevice = deviceId;
        selectedWorkflow = "point-click";
        selectedVersion = 0;
        workflowOpen = true;
        tab = "workflows";
        pointDialog?.close();
      },
      "点击位置已保存；可设置次数和间隔",
    );
  }
  function stopAllRuns() {
    void action(
      "stop-all",
      () =>
        Promise.all(
          activeRuns.map((run) => api(`/runs/${run.id}/stop`, "POST")),
        ),
      "已请求停止全部任务",
    );
  }
  async function startFlow(trial: boolean) {
    if (!flow || working || launchBlocked) return;
    if (flow.id === "swipe") {
      localStorage.setItem(
        "swipe-settings",
        JSON.stringify({ minutes: swipeMinutes, interval: swipeInterval }),
      );
    }
    await action(
      "start",
      async () => {
        const r = await api<Run>("/runs", "POST", {
          device_id: targetDevice,
          workflow_id: flow.id,
          version: flow.version,
          trial,
          tap: flow.nodes.some((n) =>
            ["tap_loop", "fixed_tap_loop"].includes(n.type),
          )
            ? { count: Number(tapCount), interval: Number(tapInterval) }
            : null,
          swipe:
            flow.id === "swipe"
              ? {
                  minutes: Number(swipeMinutes),
                  interval: Number(swipeInterval),
                }
              : null,
        });
        selectedRun = r.id;
        tab = "runs";
      },
      trial ? "试跑已开始；通过后自动启用" : "任务已开始",
    );
  }
  function reconcileRunSelection() {
    if (!runs.some((r) => r.id === selectedRun)) {
      selectedRun = runs[0]?.id ?? "";
      events = [];
    }
  }
  async function loadRunEvents(id: string) {
    const nextEvents = await api<Event[]>(`/runs/${id}/events`);
    if (alive && selectedRun === id) events = nextEvents;
  }
  async function selectRun(id: string) {
    selectedRun = id;
    events = [];
    try {
      await loadRunEvents(id);
    } catch (e) {
      if (selectedRun === id)
        error = e instanceof Error ? e.message : "读取记录失败";
    }
  }
  async function requestClear(target?: Run) {
    if (working || !connected || (target && busy(target))) return;
    const targets = target ? [target] : finishedRuns;
    if (!targets.length) return;
    clearRequest = {
      ids: targets.map((r) => r.id),
      description: target
        ? `${target.workflow.name} · ${name(target.device_id)} · ${time(target.started_at)}`
        : `将清理当前已结束的 ${targets.length} 条记录，包含已完成、已停止和需要处理的任务。`,
    };
    await tick();
    clearDialog?.showModal();
  }
  async function clearRuns() {
    if (!clearRequest || working) return;
    const ids = [...clearRequest.ids];
    working = "clear-runs";
    error = "";
    notice = "";
    const cleared = new Set<string>();
    let failed = 0;
    let failure = "";
    try {
      for (const id of ids) {
        try {
          await api(`/runs/${id}`, "DELETE");
          cleared.add(id);
        } catch (e) {
          failed += 1;
          if (!failure) failure = e instanceof Error ? e.message : "操作失败";
        }
      }
      runs = runs.filter((r) => !cleared.has(r.id));
      reconcileRunSelection();
      clearDialog?.close();
      if (cleared.size) notice = `已清理 ${cleared.size} 条记录`;
      if (failed) error = `有 ${failed} 条未清理：${failure}`;
      await load();
      await tick();
      if (cleared.size)
        (
          runList?.querySelector<HTMLButtonElement>("button.selected") ??
          emptyRunButton
        )?.focus();
    } finally {
      working = "";
    }
  }
  async function exportRun(id: string) {
    await action(
      "export",
      async () => {
        const data = await api(`/runs/${id}/export`);
        const url = URL.createObjectURL(
          new Blob([JSON.stringify(data, null, 2)], {
            type: "application/json",
          }),
        );
        const a = document.createElement("a");
        a.href = url;
        a.download = `任务记录-${id.slice(0, 8)}.json`;
        a.click();
        URL.revokeObjectURL(url);
      },
      "记录已导出",
    );
  }
  function editSchedule(s: Schedule) {
    editingSchedule = s.id;
    scheduleName = s.name;
    scheduleDevice = s.device_id;
    scheduleWorkflow = s.workflow_id;
    scheduleEnabled = s.enabled;
    scheduleTapCount = s.tap?.count ?? 10;
    scheduleTapInterval = s.tap?.interval ?? 5;
    scheduleSwipeMinutes = s.swipe?.minutes ?? 15;
    scheduleSwipeInterval = s.swipe?.interval ?? 25;
    ruleKind = s.rule.kind;
    if (s.rule.kind === "once")
      ruleDate = new Date(new Date(s.rule.at).getTime() + 8 * 3600000)
        .toISOString()
        .slice(0, 16);
    else if (s.rule.kind === "interval") ruleMinutes = s.rule.minutes;
    else {
      ruleTime = s.rule.time;
      if (s.rule.kind === "weekly") ruleDays = [...s.rule.days];
    }
  }
  function rule(): Rule {
    if (ruleKind === "once")
      return {
        kind: "once",
        at: new Date(`${ruleDate}:00+08:00`).toISOString(),
      };
    if (ruleKind === "weekly")
      return { kind: "weekly", time: ruleTime, days: ruleDays };
    if (ruleKind === "interval")
      return { kind: "interval", minutes: Number(ruleMinutes) };
    return { kind: "daily", time: ruleTime };
  }
  function ruleLabel(r: Rule) {
    if (r.kind === "once") return "执行一次";
    if (r.kind === "interval") return `每 ${r.minutes} 分钟`;
    if (r.kind === "weekly")
      return `${r.days.map((d) => ["", "周一", "周二", "周三", "周四", "周五", "周六", "周日"][d]).join("、")} ${r.time}`;
    return `每天 ${r.time}`;
  }
  function stepTitle(workflow: Workflow, id: string) {
    return workflow.nodes.find((node) => node.id === id)?.label || id;
  }
  function stepNumber(workflow: Workflow, id: string) {
    return String(
      workflow.nodes.findIndex((node) => node.id === id) + 1,
    ).padStart(2, "0");
  }
  function name(id: string) {
    return devices.find((d) => d.id === id)?.name || id;
  }
  onMount(() => {
    alive = true;
    theme =
      document.documentElement.dataset.theme === "dark" ? "dark" : "light";
    const updateClock = () => {
      clock = new Intl.DateTimeFormat("zh-CN", {
        timeZone: "Asia/Shanghai",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      }).format(new Date());
    };
    updateClock();
    const clockTimer = setInterval(updateClock, 1000);
    try {
      const saved = JSON.parse(
        localStorage.getItem("swipe-settings") || "null",
      );
      if (saved && saved.minutes > 0 && saved.interval > 0) {
        swipeMinutes = saved.minutes;
        swipeInterval = saved.interval;
      }
    } catch {
      /* 使用默认设置 */
    }
    let socket: WebSocket | undefined;
    let retry: ReturnType<typeof setTimeout> | undefined;
    async function connect() {
      try {
        await session();
        await load();
        if (!alive) return;
        socket = new WebSocket(`ws://${location.host}/api/ws`);
        socket.onmessage = () => void load();
        socket.onclose = () => {
          connected = false;
          if (alive) retry = setTimeout(() => void connect(), 3000);
        };
        socket.onerror = () => {
          connected = false;
        };
      } catch {
        connected = false;
        loading = false;
        if (alive) retry = setTimeout(() => void connect(), 3000);
      }
    }
    void connect();
    return () => {
      alive = false;
      clearTimeout(retry);
      clearInterval(clockTimer);
      socket?.close();
      Object.values(pictures).forEach(URL.revokeObjectURL);
    };
  });
</script>

<svelte:head
  ><title>ADB 工作台 · 手机自动化</title><meta
    name="description"
    content="手机、工作流与定时任务的本地控制台"
  /></svelte:head
>
<div class="shell">
  <aside class="sidebar">
    <a class="brand" href="/" aria-label="ADB 工作台首页"
      ><span class="brand-icon" aria-hidden="true">⌁</span><span
        class="brand-words"
        >pulse<span class="brand-accent">.</span> studio<small
          >ANDROID AUTOMATION</small
        ></span
      ></a
    >
    <div class="nav-label">工作空间</div>
    <nav aria-label="主要功能">
      {#each tabs as item}<button
          aria-label={item.label}
          title={item.label}
          class:current={tab === item.id}
          onclick={() => {
            tab = item.id;
            selectedDeviceId = "";
            workflowOpen = false;
            notice = "";
            error = "";
          }}
          aria-current={tab === item.id ? "page" : undefined}
          ><item.icon size={21} /><span>{item.label}</span
          >{#if item.id === "runs" && activeRuns.length}<span class="nav-count"
              >{activeRuns.length}</span
            >{/if}</button
        >{/each}
    </nav>
    <div class="sidebar-foot">
      <div>
        <span class="local-dot"></span>本地服务 · {connected
          ? "已连接"
          : "连接中"}
      </div>
      <small>ADB 工作台 · 网页关闭后继续运行</small>
    </div>
  </aside>
  <main>
    <header class="topbar">
      <div class="breadcrumb">
        工作空间 <span>/</span><strong
          >{tab === "devices" && selectedDeviceId
            ? "设备详情"
            : tab === "workflows" && workflowOpen
              ? "任务详情"
              : tabs.find((item) => item.id === tab)?.label}</strong
        >
      </div>
      <div class="top-actions">
        <span class="mode-tag" class:disconnected={!connected}
          >{connected ? "LOCAL" : "连接中"}</span
        >
        <button
          class="theme-button"
          onclick={toggleTheme}
          aria-label={theme === "light" ? "切换夜间模式" : "切换日间模式"}
          >{#if theme === "light"}<Moon
              size={12}
              aria-hidden="true"
            />夜间模式{:else}<Sun
              size={12}
              aria-hidden="true"
            />日间模式{/if}</button
        >
        <time class="clock">{clock}</time><span
          class="avatar"
          aria-hidden="true">PS</span
        >
      </div>
    </header>
    <section class="content" bind:this={contentElement}>
      {#if error}<div class="message error" role="alert">
          {error}<button aria-label="关闭提示" onclick={() => (error = "")}
            >×</button
          >
        </div>{/if}
      {#if notice}<div class="message success" role="status">
          <Check size={18} />{notice}<button
            aria-label="关闭提示"
            onclick={() => (notice = "")}>×</button
          >
        </div>{/if}
      {#if tab !== "devices" && tab !== "workflows"}<div class="page-heading">
          <div>
            <p class="eyebrow">手机自动化 · 本地托管</p>
            <h1>
              {tabs.find((t) => t.id === tab)?.label}
            </h1>
            <p class="intro">
              {(
                {
                  schedules: "设置执行时间，交给后台完成。",
                  runs: "任务进度、结果与判断依据。",
                  settings: "后台状态与识别模型。",
                } as Record<string, string>
              )[tab]}
            </p>
          </div>
          {#if activeRuns.length}<Button
              variant="outline"
              class="stop-button"
              disabled={!!working}
              onclick={stopAllRuns}><Square size={16} />全部停止</Button
            >{/if}
        </div>
      {/if}
      {#if tab === "devices"}
        <DeviceManager
          {devices}
          {runs}
          {pictures}
          {working}
          {loading}
          {connected}
          {lastUpdated}
          bind:selectedDeviceId
          onRefresh={() => {
            void action(
              "refresh",
              () => api("/devices/refresh", "POST"),
              "已刷新手机状态",
            );
          }}
          onCapture={(device) => {
            void capture(device);
          }}
          onRename={(device) => {
            editDevice = device;
            rename = device.name;
          }}
          onScreen={(device, operation) => {
            void controlScreen(device, operation);
          }}
          onAwake={(device, enabled) => {
            void action(
              `awake-${device.id}`,
              () =>
                api(`/devices/${device.id}`, "PUT", { keep_awake: enabled }),
              "常亮设置已保存",
            );
          }}
          onMarkPoint={(device) => {
            void markPoint(device);
          }}
          onChooseTask={(device) => {
            targetDevice = device.id;
            selectedWorkflow = "swipe";
            selectedVersion = 0;
            workflowOpen = true;
            tab = "workflows";
          }}
          onViewRuns={(device) => {
            const latest = runs.find((run) => run.device_id === device.id);
            selectedRun = latest?.id || "";
            tab = "runs";
            if (latest) void selectRun(latest.id);
          }}
          onStopAll={stopAllRuns}
        />
      {:else if tab === "workflows"}
        <div hidden={workflowOpen && !!flow}>
          <TaskList
            {workflows}
            {devices}
            {runs}
            {schedules}
            {loading}
            {connected}
            {working}
            onOpen={(workflow) => {
              selectedWorkflow = workflow.id;
              selectedVersion = workflow.version;
              workflowOpen = true;
              notice = "";
              error = "";
            }}
            onRefresh={() => {
              void action("refresh-tasks", load, "任务已刷新");
            }}
            onViewRuns={(run) => {
              tab = "runs";
              void selectRun(run.id);
            }}
            onStop={(run) => {
              void action(
                `stop-${run.id}`,
                () => api(`/runs/${run.id}/stop`, "POST"),
                "已请求停止任务",
              );
            }}
            onStopAll={stopAllRuns}
          />
        </div>
        {#if workflowOpen && flow}
          <div class="page-heading">
            <div>
              <h1>任务详情</h1>
              <p class="intro">选择手机与参数，再试跑或运行。</p>
            </div>
            <div class="page-actions">
              {#if activeRuns.length}<Button
                  variant="outline"
                  class="stop-button"
                  disabled={!!working}
                  onclick={stopAllRuns}><Square size={14} />全部停止</Button
                >{/if}
              <Button
                variant="outline"
                onclick={() => {
                  workflowOpen = false;
                  notice = "";
                  error = "";
                }}><ArrowLeft size={14} />返回任务</Button
              >
            </div>
          </div>
          <article class="panel flow-panel">
            <div class="panel-heading">
              <div>
                <h2>{flow.name}</h2>
                <p>
                  {flow.id === "swipe"
                    ? "保持当前页面，按下方时长和间隔滑动。"
                    : flow.description}
                </p>
              </div>
              <Badge
                class="status"
                variant={flow.active ? "default" : "outline"}
                >{flow.active ? "已启用" : "待试跑"}</Badge
              >
            </div>
            <form
              class="task-launch"
              onsubmit={(event) => {
                event.preventDefault();
                void startFlow(
                  (event.submitter as HTMLButtonElement | null)?.value !==
                    "run",
                );
              }}
            >
              {#if flow.id === "point-click"}
                <div class="card-actions">
                  <Button
                    variant="outline"
                    disabled={!!working ||
                      !devices.find((d) => d.id === targetDevice)?.online}
                    onclick={() => {
                      const d = devices.find((d) => d.id === targetDevice);
                      if (d) void markPoint(d);
                    }}>标注点击位置</Button
                  >
                </div>
                <p class="hint">
                  {devices.find((d) => d.id === targetDevice)?.tap_point
                    ? `已保存位置：${devices.find((d) => d.id === targetDevice)?.tap_point?.x}，${devices.find((d) => d.id === targetDevice)?.tap_point?.y}`
                    : "请选择手机并标注位置"}
                </p>
              {/if}
              {#if flow.nodes.some( (n) => ["tap_loop", "fixed_tap_loop"].includes(n.type) )}<div
                  class="flow-controls"
                >
                  <label
                    >点击次数<input
                      type="number"
                      min="1"
                      max="10000"
                      step="1"
                      required
                      bind:value={tapCount}
                    /></label
                  >
                  <label
                    >点击间隔（秒）<input
                      type="number"
                      min="1"
                      max="86400"
                      step="any"
                      required
                      bind:value={tapInterval}
                    /></label
                  >
                </div>
                <p class="hint">
                  从当前页面开始；先点击，再等待间隔。按已确认的位置直接点击，不识别或校验画面。请保持所标注屏幕及当前页面。
                </p>{/if}
              {#if flow.id === "swipe"}<div class="flow-controls">
                  <label
                    >总时长（分钟）<input
                      type="number"
                      min="0.01"
                      max="10080"
                      step="any"
                      required
                      bind:value={swipeMinutes}
                    /></label
                  >
                  <label
                    >滑动间隔（秒）<input
                      type="number"
                      min="0.01"
                      max="86400"
                      step="any"
                      required
                      bind:value={swipeInterval}
                    /></label
                  >
                </div>
                <p class="hint">
                  先等待，再滑动；到达总时长自动结束。以上设置用于本次运行。
                </p>{/if}
              <div class="flow-controls">
                <label
                  >版本<select
                    value={flow.version}
                    onchange={(e) =>
                      (selectedVersion = Number(e.currentTarget.value))}
                    >{#each workflows.filter((w) => w.id === flow.id) as w}<option
                        value={w.version}
                        >第 {w.version} 版{w.active ? " · 已启用" : ""}</option
                      >{/each}</select
                  ></label
                ><label
                  >执行手机<select bind:value={targetDevice}
                    >{#each devices.filter((d) => !flow.devices.length || flow.devices.includes(d.id)) as d}<option
                        value={d.id}
                        >{d.name} · {d.online ? "在线" : "离线"}</option
                      >{/each}</select
                  ></label
                ><Button
                  disabled={!!working || !!launchBlocked}
                  type="submit"
                  name="mode"
                  value="trial"><Play size={16} />试跑此版本</Button
                >{#if flow.active}<Button
                    variant="outline"
                    disabled={!!working || !!launchBlocked}
                    type="submit"
                    name="mode"
                    value="run">直接运行</Button
                  >{/if}{#if !flow.active && runs.some((r) => r.trial && r.state === "succeeded" && r.workflow.id === flow.id && r.workflow.version === flow.version)}<Button
                    variant="outline"
                    onclick={() =>
                      action(
                        "publish",
                        () =>
                          api(
                            `/workflows/${flow.id}@${flow.version}/publish`,
                            "POST",
                          ),
                        "此版本已启用",
                      )}>启用 / 回退到此版</Button
                  >{/if}
              </div>
              {#if launchBlocked}<p class="hint" role="status">
                  {launchBlocked}
                </p>{/if}
              <p class="hint">
                试跑通过后自动启用。Agent 修改步骤；你查看流程、管理运行和定时。
              </p>
            </form>
            <ol class="flow-nodes">
              {#each flow.nodes as n, i}
                {@const nodeRun =
                  launchRun?.workflow.id === flow.id &&
                  launchRun.workflow.version === flow.version &&
                  launchRun.node === n.id
                    ? launchRun
                    : null}
                <li class:executing={!!nodeRun}>
                  <div class="node-card">
                    <span class="step-no">{String(i + 1).padStart(2, "0")}</span
                    >
                    <div>
                      <small>步骤 {String(i + 1).padStart(2, "0")}</small>
                      <h3>
                        {flow.id === "swipe" && n.type === "swipe_loop"
                          ? `每隔 ${swipeInterval} 秒上滑，运行 ${swipeMinutes} 分钟`
                          : n.label}
                      </h3>
                      {#if n.type === "qishui_loop"}<div class="branches">
                          <span>金币增长 → 继续播放</span><span
                            >暂停 {String(n.stall)} 秒 → 上滑</span
                          ><span
                            >{String(n.finish_stall)} 秒无增长 + {String(
                              n.finish_swipes,
                            )} 次无法切换 → 复查完成</span
                          ><span>加载 / 网络异常 → 等待</span>
                        </div>{:else if n.type === "branch"}<div
                          class="branches"
                        >
                          {#each n.cases || [] as c}<span
                              >{c.contains} → {stepTitle(flow, c.next)}</span
                            >{/each}<span
                            >其他 → {stepTitle(flow, String(n.otherwise))}</span
                          >
                        </div>{:else if n.type === "qishui_navigate"}<p>
                          识别金币入口、关闭遮挡，点任务文字，确认视频页。
                        </p>{/if}
                    </div>
                    {#if nodeRun}<Badge>{runState(nodeRun.state)}</Badge>{/if}
                  </div>
                  {#if n.next}<div class="flow-arrow">
                      <ArrowDown size={20} /><span
                        >下一步 · {stepTitle(flow, n.next)}</span
                      >
                    </div>{/if}
                </li>{/each}
            </ol>
            <div class="flow-footer">
              适用：{flow.devices.length
                ? flow.devices.map(name).join("、")
                : "所有已登记手机"}
              <span>模型上限：{flow.model_limit} 次 / 运行</span>
            </div>
          </article>
        {/if}
      {:else if tab === "schedules"}
        <div class="schedule-layout">
          <form
            class="panel"
            onsubmit={(e) => {
              e.preventDefault();
              void action(
                "schedule",
                () =>
                  api(
                    editingSchedule
                      ? `/schedules/${editingSchedule}`
                      : "/schedules",
                    editingSchedule ? "PUT" : "POST",
                    {
                      name: scheduleName || "定时任务",
                      device_id: scheduleDevice,
                      workflow_id: scheduleWorkflow,
                      rule: rule(),
                      enabled: scheduleEnabled,
                      tap: activeWorkflows
                        .find((w) => w.id === scheduleWorkflow)
                        ?.nodes.some((n) =>
                          ["tap_loop", "fixed_tap_loop"].includes(n.type),
                        )
                        ? {
                            count: Number(scheduleTapCount),
                            interval: Number(scheduleTapInterval),
                          }
                        : null,
                      swipe:
                        scheduleWorkflow === "swipe"
                          ? {
                              minutes: Number(scheduleSwipeMinutes),
                              interval: Number(scheduleSwipeInterval),
                            }
                          : null,
                    },
                  ),
                "定时已保存",
              );
            }}
          >
            <h2>
              <Plus size={21} />{editingSchedule ? "调整定时" : "新建定时"}
            </h2>
            <label
              >名称<input
                bind:value={scheduleName}
                placeholder="例如：早上刷视频"
                maxlength="80"
              /></label
            ><label
              >手机<select bind:value={scheduleDevice}
                >{#each devices as d}<option value={d.id}>{d.name}</option
                  >{/each}</select
              ></label
            ><label
              >工作流<select bind:value={scheduleWorkflow}
                >{#each activeWorkflows as w}<option value={w.id}
                    >{w.name}</option
                  >{/each}</select
              ></label
            >{#if activeWorkflows
              .find((w) => w.id === scheduleWorkflow)
              ?.nodes.some( (n) => ["tap_loop", "fixed_tap_loop"].includes(n.type) )}
              <label
                >点击次数<input
                  type="number"
                  min="1"
                  max="10000"
                  step="1"
                  required
                  bind:value={scheduleTapCount}
                /></label
              >
              <label
                >点击间隔（秒）<input
                  type="number"
                  min="1"
                  max="86400"
                  step="any"
                  required
                  bind:value={scheduleTapInterval}
                /></label
              >
            {/if}{#if scheduleWorkflow === "swipe"}
              <label
                >总时长（分钟）<input
                  type="number"
                  min="0.01"
                  max="10080"
                  step="any"
                  required
                  bind:value={scheduleSwipeMinutes}
                /></label
              >
              <label
                >滑动间隔（秒）<input
                  type="number"
                  min="0.01"
                  max="86400"
                  step="any"
                  required
                  bind:value={scheduleSwipeInterval}
                /></label
              >
            {/if}{#if !activeWorkflows.length}<p class="hint">
                先试跑工作流，通过后即可设置定时。
              </p>{/if}<label
              >执行规则<select bind:value={ruleKind}
                ><option value="daily">每天</option><option value="weekly"
                  >每周</option
                ><option value="interval">固定间隔</option><option value="once"
                  >只执行一次</option
                ></select
              ></label
            >{#if ruleKind === "once"}<label
                >日期与时间<input
                  type="datetime-local"
                  required
                  bind:value={ruleDate}
                /></label
              >{:else if ruleKind === "interval"}<label
                >间隔（分钟）<input
                  type="number"
                  min="1"
                  max="525600"
                  required
                  bind:value={ruleMinutes}
                /></label
              >{:else}<label
                >时间<input type="time" required bind:value={ruleTime} /></label
              >{/if}{#if ruleKind === "weekly"}<fieldset>
                <legend>星期</legend>
                <div class="weekdays">
                  {#each ["一", "二", "三", "四", "五", "六", "日"] as day, i}<label
                      ><input
                        type="checkbox"
                        checked={ruleDays.includes(i + 1)}
                        onchange={(e) =>
                          (ruleDays = e.currentTarget.checked
                            ? [...ruleDays, i + 1]
                            : ruleDays.filter((d) => d !== i + 1))}
                      />周{day}</label
                    >{/each}
                </div>
              </fieldset>{/if}
            <label class="awake"
              ><span>开启定时</span><input
                type="checkbox"
                class="switch"
                bind:checked={scheduleEnabled}
              /></label
            >
            <p class="hint">
              上海时间。手机离线时，本次跳过；正在执行的任务掉线后等待上线。
            </p>
            <Button
              type="submit"
              disabled={!!working || !activeWorkflows.length || !connected}
              ><CalendarClock size={17} />{editingSchedule
                ? "保存修改"
                : "保存定时"}</Button
            >
          </form>
          <div class="schedule-list">
            {#if !schedules.length}<div class="empty">
                <CalendarClock size={32} />
                <h3>还没有定时</h3>
                <p>选择手机和工作流，设置执行时间。</p>
              </div>{/if}{#each schedules as s}<article class="panel">
                <div class="panel-heading">
                  <h2>{s.name}</h2>
                  <Badge variant="outline"
                    >{s.enabled ? "已开启" : "已暂停"}</Badge
                  >
                </div>
                <p>
                  {name(s.device_id)} · {workflows.find(
                    (w) => w.id === s.workflow_id,
                  )?.name}
                </p>
                <strong class="schedule-rule">{ruleLabel(s.rule)}</strong>
                <p>下次：{s.enabled ? time(s.next_at) : "暂停中"}</p>
                {#if s.last_result}<p class="hint">
                    最近：{s.last_result}
                  </p>{/if}
                <div class="card-actions">
                  <Button
                    variant="outline"
                    disabled={!!working}
                    onclick={() => editSchedule(s)}>修改</Button
                  >
                  <Button
                    variant="outline"
                    disabled={!!working}
                    onclick={() =>
                      action(
                        "toggle-schedule",
                        () =>
                          api(`/schedules/${s.id}`, "PUT", {
                            ...s,
                            enabled: !s.enabled,
                          }),
                        "定时已更新",
                      )}>{s.enabled ? "暂停" : "开启"}</Button
                  ><Button
                    variant="ghost"
                    disabled={!!working}
                    onclick={() =>
                      action(
                        "delete-schedule",
                        () => api(`/schedules/${s.id}`, "DELETE"),
                        "定时已删除",
                      )}>删除</Button
                  >
                </div>
              </article>{/each}
          </div>
        </div>
      {:else if tab === "runs"}
        <div class="run-toolbar">
          <p aria-live="polite">
            共 <strong>{runs.length}</strong> 条
            <span
              >进行中 {activeRuns.length} · 已结束 {finishedRuns.length}</span
            >
          </p>
          <Button
            variant="outline"
            disabled={!!working || !connected || !finishedRuns.length}
            onclick={() => requestClear()}
            ><Trash2 size={17} aria-hidden="true" />清理已结束</Button
          >
        </div>
        {#if !runs.length}
          <div class="panel empty">
            {#if loading}<LoaderCircle size={32} class="spin" />
              <h3>正在读取记录</h3>
            {:else}<History size={32} aria-hidden="true" />
              <h3>暂无运行记录</h3>
              <p>开始任务后，可在这里查看进度和结果。</p>
              <Button
                bind:ref={emptyRunButton}
                onclick={() => (tab = "workflows")}>选择工作流</Button
              >
            {/if}
          </div>
        {:else}<div class="run-layout">
            <div class="run-list" bind:this={runList}>
              {#each runs as r (r.id)}<button
                  class:selected={selectedRun === r.id}
                  aria-pressed={selectedRun === r.id}
                  onclick={() => void selectRun(r.id)}
                  ><div>
                    <strong>{r.workflow.name}</strong><small
                      >{name(r.device_id)} · {time(r.started_at)}</small
                    >
                  </div>
                  <Badge
                    variant={r.state === "needs_help"
                      ? "destructive"
                      : "outline"}>{runState(r.state)}</Badge
                  ></button
                >{/each}
            </div>
            {#if run}<article class="panel">
                <div class="panel-heading">
                  <div>
                    <h2>{run.workflow.name}</h2>
                    <p>
                      {name(run.device_id)} · 第 {run.workflow.version} 版{run.trial
                        ? " · 试跑"
                        : ""}
                    </p>
                  </div>
                  <Badge
                    variant={run.state === "needs_help"
                      ? "destructive"
                      : "default"}>{runState(run.state)}</Badge
                  >
                </div>
                <div class="card-actions run-actions">
                  {#if busy(run)}<Button
                      variant="destructive"
                      disabled={!!working}
                      onclick={() =>
                        action(
                          "stop",
                          () => api(`/runs/${run.id}/stop`, "POST"),
                          "已请求停止",
                        )}><Square size={16} />停止任务</Button
                    >{/if}<Button
                    variant="outline"
                    disabled={!!working}
                    onclick={() => exportRun(run.id)}
                    ><Download size={17} />导出记录</Button
                  >
                  {#if !busy(run)}<Button
                      variant="outline"
                      disabled={!!working || !connected}
                      onclick={() => requestClear(run)}
                      ><Trash2
                        size={17}
                        aria-hidden="true"
                      />清理这条记录</Button
                    >{/if}
                </div>
                <div class="run-reason" aria-live="polite">{run.reason}</div>
                <div class="run-meta">
                  <span>当前步骤：{stepTitle(run.workflow, run.node)}</span
                  ><span
                    >模型：{run.model_calls} / {run.workflow.model_limit} 次</span
                  >
                </div>
                {#if run.checkpoint.coin !== undefined}<p class="hint">
                    金币：{String(run.checkpoint.coin ?? "待识别")} · 连续无增长：{String(
                      run.checkpoint.idle ?? 0,
                    )} 秒 · 已复查：{String(run.checkpoint.failed_swipes ?? 0)} 次
                  </p>{:else if run.checkpoint.done !== undefined}<p
                    class="hint"
                  >
                    已滑动 {String(run.checkpoint.done)} 次
                  </p>{/if}
                <h3 class="event-heading">判断与行动</h3>
                <ol class="event-list">
                  {#each events as ev}<li>
                      <time>{time(ev.at, true)}</time>
                      <div>
                        <small>步骤 {stepNumber(run.workflow, ev.node)}</small>
                        <p>{ev.message}</p>
                      </div>
                    </li>{/each}
                </ol>
                {#if run.last_shot}<details open>
                    <summary>最近关键画面</summary><img
                      class="preview"
                      src={`/api/shots/${run.last_shot}`}
                      alt="任务最近关键画面"
                    />
                  </details>{/if}
              </article>{/if}
          </div>{/if}
      {:else if tab === "settings"}
        <div class="settings-layout">
          <article class="panel">
            <h2>后台运行</h2>
            <dl class="settings-info">
              <div>
                <dt>访问位置</dt>
                <dd>{settings?.host || "127.0.0.1:17841"}</dd>
              </div>
              <div>
                <dt>登录自启</dt>
                <dd>{settings?.autostart ? "已安装" : "尚未安装"}</dd>
              </div>
              <div>
                <dt>Mac 防休眠</dt>
                <dd>使用你的系统设置</dd>
              </div>
              <div>
                <dt>时间</dt>
                <dd>上海时间</dd>
              </div>
              <div>
                <dt>记录保留</dt>
                <dd>30 天</dd>
              </div>
            </dl>
            <p class="hint">
              关闭网页后，任务继续运行。密钥保存在本机，Agent 使用独立访问令牌。
            </p>
          </article>
          <form
            class="panel"
            onsubmit={(e) => {
              e.preventDefault();
              void action(
                "model",
                () => api("/settings/model", "PUT", model),
                "模型设置已保存",
              );
            }}
          >
            <h2>辅助识别模型</h2>
            <p class="hint">
              先使用页面控件、文字和图片规则；无法判断时调用模型。
            </p>
            <label>模型名称<input required bind:value={model.model} /></label
            ><label
              >接口地址<input
                type="url"
                required
                bind:value={model.endpoint}
              /></label
            ><label
              >接口格式<select bind:value={model.api}
                ><option value="responses">Responses</option><option
                  value="chat_completions">Chat Completions</option
                ></select
              ></label
            ><label>密钥变量<input required bind:value={model.key_env} /></label
            >
            <p class="hint">
              默认复用现有 MODEL_KEY。密钥内容不会显示在网页或记录中。
            </p>
            <Button type="submit" disabled={!!working || !connected}
              >保存模型设置</Button
            >
          </form>
        </div>
      {/if}
    </section>
  </main>
</div>
<dialog
  bind:this={clearDialog}
  class="modal clear-modal"
  aria-labelledby="clear-title"
  aria-describedby="clear-description clear-hint"
  onclose={() => (clearRequest = null)}
  oncancel={(e) => {
    if (working === "clear-runs") e.preventDefault();
  }}
>
  {#if clearRequest}
    <h2 id="clear-title">
      清理{clearRequest.ids.length === 1 ? "这条记录" : "已结束的记录"}？
    </h2>
    <p id="clear-description">{clearRequest.description}</p>
    <p id="clear-hint" class="hint">
      清理后从列表移除，本机仍保留归档和截图。进行中的任务会保留。
    </p>
    <div class="card-actions">
      <Button
        variant="outline"
        disabled={working === "clear-runs"}
        onclick={() => clearDialog?.close()}>取消</Button
      >
      <Button variant="destructive" disabled={!!working} onclick={clearRuns}
        >{#if working === "clear-runs"}<LoaderCircle
            size={17}
            class="spin"
            aria-hidden="true"
          />清理中…{:else}确认清理{/if}</Button
      >
    </div>
  {/if}
</dialog>
<dialog
  bind:this={renameDialog}
  onclose={() => (editDevice = null)}
  aria-labelledby="rename-title"
  class="modal"
>
  {#if editDevice}<h2 id="rename-title">重命名手机</h2>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        const id = editDevice!.id;
        void action(
          "rename",
          async () => {
            await api(`/devices/${id}`, "PUT", { name: rename });
            editDevice = null;
          },
          "手机已重命名",
        );
      }}
    >
      <label
        >手机名称<input bind:value={rename} required maxlength="40" /></label
      >
      <div class="card-actions">
        <Button type="submit" disabled={!!working}>保存名称</Button><Button
          variant="outline"
          onclick={() => (editDevice = null)}>取消</Button
        >
      </div>
    </form>{/if}
</dialog>

<dialog
  bind:this={pointDialog}
  class="modal point-modal"
  aria-labelledby="point-title"
>
  <h2 id="point-title">标注点击位置 · {pointDevice?.name}</h2>
  <p>点一下截图中的目标位置，再点击“确定位置”。此操作不会点击手机。</p>
  {#if pointImage}<button
      class="point-image"
      type="button"
      aria-label="在手机截图上选择点击位置"
      onclick={choosePoint}
    >
      <img
        src={pointImage}
        alt="用于标注位置的手机截图"
        onload={(e) => {
          pointWidth = (e.currentTarget as HTMLImageElement).naturalWidth;
          pointHeight = (e.currentTarget as HTMLImageElement).naturalHeight;
        }}
      />
      {#if pendingPoint}<span
          class="point-marker"
          style:left={`${(pendingPoint.x / pendingPoint.width) * 100}%`}
          style:top={`${(pendingPoint.y / pendingPoint.height) * 100}%`}
        ></span>{/if}
    </button>{/if}
  <p aria-live="polite">
    {pendingPoint
      ? `点击位置：${pendingPoint.x}，${pendingPoint.y}（屏幕 ${pendingPoint.width} × ${pendingPoint.height}）`
      : "尚未选择位置"}
  </p>
  <div class="card-actions">
    <Button disabled={!!working || !pendingPoint} onclick={confirmPoint}
      >确定位置</Button
    ><Button variant="outline" onclick={() => pointDialog?.close()}>取消</Button
    >
  </div>
</dialog>
