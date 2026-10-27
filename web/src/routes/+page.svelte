<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { replaceState } from "$app/navigation";
  import { page } from "$app/state";
  import {
    readHomeNavigation,
    homeNavigationUrl,
    type HomeTab,
  } from "$lib/home-navigation";
  import {
    Smartphone,
    Workflow as FlowIcon,
    CalendarClock,
    History,
    Settings as SettingsIcon,
    Play,
    Hand,
    Square,
    ArrowLeft,
    ArrowDown,
    ChevronDown,
    Download,
    LoaderCircle,
    Trash2,
    Sun,
    Moon,
  } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import VisionInspector from "$lib/components/VisionInspector.svelte";
  import DeviceManager from "$lib/components/DeviceManager.svelte";
  import TaskList from "$lib/components/TaskList.svelte";
  import ScheduleManager from "$lib/components/ScheduleManager.svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import RunFeedback from "$lib/components/RunFeedback.svelte";
  import { notify } from "$lib/notifications";
  import { latestWorkflows } from "$lib/tasks";
  import { resumeBlocked } from "$lib/run-feedback";
  import { scheduleInput } from "$lib/schedules";
  import type { Draft } from "$lib/workflow-editor";
  import Badge from "$lib/components/ui/badge/badge.svelte";
  import {
    api,
    session,
    photo,
    busy,
    time,
    runLabel,
    type Device,
    type TapPoint,
    type Workflow,
    type Run,
    type Schedule,
    type Settings,
    type Event,
  } from "$lib/api";

  const tabs = [
    { id: "devices", label: "设备管理", icon: Smartphone },
    { id: "workflows", label: "自动化任务", icon: FlowIcon },
    { id: "schedules", label: "定时任务", icon: CalendarClock },
    { id: "runs", label: "执行记录", icon: History },
    { id: "settings", label: "系统设置", icon: SettingsIcon },
  ] as const;
  let tab = $state<HomeTab>("devices");
  let selectedDeviceId = $state("");
  let deviceDetailTab = $state<"parameters" | "task">("parameters");
  let taskSetupDevice = $state("");
  let taskSettingsExpanded = $state(true);
  let taskSettingsContext = $state("");
  let deviceEvents = $state<Event[]>([]);
  let deviceEventsId = $state("");
  let deviceEventsError = $state("");
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
  let drafts = $state<Draft[]>([]);
  let runs = $state<Run[]>([]);
  let schedules = $state<Schedule[]>([]);
  let settings = $state<Settings | null>(null);
  let connected = $state(false);
  let loading = $state(true);
  let working = $state("");
  let loadError = "";
  let renameDialog = $state<HTMLDialogElement>();
  let editDevice = $state<Device | null>(null);
  let rename = $state("");
  let pictures = $state<Record<string, string>>({});
  let selectedWorkflow = $state("swipe");
  let selectedVersion = $state(0);
  let targetDevice = $state("3430131496001EB");
  let pointDialog = $state<HTMLDialogElement>();
  let pointDevice = $state<Device | null>(null);
  let pointOrigin = $state("workflows");
  let pointImage = $state("");
  let pendingPoint = $state<TapPoint | null>(null);
  let pointWidth = $state(0);
  let pointHeight = $state(0);
  let tapCount = $state(10);
  let tapInterval = $state(5);
  let swipeMinutes = $state(15);
  let swipeInterval = $state(25);
  let selectedRun = $state("");
  let navigationReady = $state(false);
  $effect(() => {
    if (!navigationReady) return;
    const navigation = {
      tab,
      selectedDeviceId,
      deviceDetailTab,
      workflowOpen,
      selectedWorkflow,
      selectedVersion,
      selectedRun,
    };
    untrack(() => {
      const next = homeNavigationUrl(page.url, navigation);
      if (next.href !== page.url.href) replaceState(next, page.state);
    });
  });
  let events = $state<Event[]>([]);
  let clearDialog = $state<HTMLDialogElement>();
  let runList = $state<HTMLDivElement>();
  let emptyRunButton = $state<HTMLElement | null>(null);
  let clearRequest = $state<{ ids: string[]; description: string } | null>(
    null,
  );
  let model = $state({
    endpoint: "",
    model: "",
    api: "responses",
    key_env: "MODEL_KEY",
  });
  const activeRuns = $derived(runs.filter(busy));
  const finishedRuns = $derived(runs.filter((r) => !busy(r)));
  const flow = $derived(
    tab === "devices" || selectedVersion === 0
      ? latestWorkflows(workflows).find((w) => w.id === selectedWorkflow)
      : workflows.find(
          (w) => w.id === selectedWorkflow && w.version === selectedVersion,
        ),
  );
  const deviceRun = $derived(
    runs.find((r) => r.device_id === selectedDeviceId && busy(r)) ||
      runs.find((r) => r.device_id === selectedDeviceId),
  );
  const selectedPhone = $derived(
    devices.find((device) => device.id === selectedDeviceId),
  );
  const pageTitle = $derived(
    selectedPhone
      ? `${selectedPhone.name} · ${deviceRun && busy(deviceRun) ? deviceRun.workflow.name : "空闲"}`
      : "手机自动化",
  );
  $effect(() => {
    if (!selectedDeviceId) return;
    const running = !!deviceRun && busy(deviceRun);
    const context = `${selectedDeviceId}:${running ? deviceRun!.id : "idle"}`;
    if (context !== taskSettingsContext) {
      taskSettingsContext = context;
      taskSettingsExpanded = !running;
    }
  });
  const taskChoices = $derived(
    latestWorkflows(workflows)
      .filter((w) => !w.archived)
      .map((workflow) => ({
        value: workflow.id,
        label: workflow.name,
      })),
  );
  $effect(() => {
    if (
      tab !== "devices" ||
      !selectedDeviceId ||
      taskSetupDevice === selectedDeviceId
    )
      return;
    taskSetupDevice = selectedDeviceId;
    const previous = deviceRun;
    selectedWorkflow = previous?.workflow.id || "swipe";
    selectedVersion = previous?.workflow.version || 0;
    const swipe = previous?.workflow.nodes.find(
      (node) => node.type === "swipe_loop",
    );
    if (typeof swipe?.minutes === "number") swipeMinutes = swipe.minutes;
    if (typeof swipe?.interval === "number") swipeInterval = swipe.interval;
    const tap = previous?.workflow.nodes.find((node) =>
      ["fixed_tap_loop", "tap_loop"].includes(node.type),
    );
    if (typeof tap?.count === "number") tapCount = tap.count;
    if (typeof tap?.interval === "number") tapInterval = tap.interval;
  });
  $effect(() => {
    const current = deviceRun;
    if (tab !== "devices" || deviceDetailTab !== "task" || !current) return;
    const controller = new AbortController();
    void api<Event[]>(
      `/runs/${current.id}/events`,
      "GET",
      undefined,
      controller.signal,
    )
      .then((items) => {
        if (controller.signal.aborted) return;
        deviceEventsError = "";
        deviceEventsId = current.id;
        deviceEvents = items;
      })
      .catch((e) => {
        if (controller.signal.aborted) return;
        const message = e instanceof Error ? e.message : "读取执行记录失败";
        if (deviceEventsError !== message) notify.error(message);
        deviceEventsError = message;
      });
    return () => controller.abort();
  });
  const launchDevice = $derived(
    devices.find((device) => device.id === targetDevice),
  );
  const launchRun = $derived(
    activeRuns.find((run) => run.device_id === targetDevice),
  );
  const launchBlocked = $derived(
    !connected
      ? "后台未连接，请稍后重试。"
      : flow?.devices.length &&
          launchDevice &&
          !flow.devices.includes(launchDevice.id)
        ? "此任务尚未适配当前手机，请选择其他任务。"
        : !launchDevice?.online
          ? "手机离线，连接后可开始。"
          : launchDevice.locked
            ? "请先在设备详情中唤醒并解锁手机。"
            : launchRun
              ? `当前任务：${runLabel(launchRun)}。停止后可开始新任务。`
              : flow?.id === "point-click" && !launchDevice.tap_point
                ? "请先标注这台手机的点击位置。"
                : settings &&
                    !settings.local_ocr &&
                    flow?.nodes.some((n) =>
                      [
                        "tap_text",
                        "tap_point",
                        "key",
                        "branch",
                        "tap_loop",
                        "qishui_navigate",
                        "qishui_loop",
                      ].includes(n.type),
                    )
                  ? "此任务需要本地文字识别，当前平台未安装。请使用已适配的纯视觉任务。"
                  : "",
  );
  $effect(() => {
    if (tab === "devices") targetDevice = selectedDeviceId;
    else if (
      flow &&
      flow.devices.length &&
      !flow.devices.includes(targetDevice)
    )
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
        const [d, w, r, s, config, savedDrafts] = await Promise.all([
          api<Device[]>("/devices"),
          api<Workflow[]>("/workflows"),
          api<Run[]>("/runs"),
          api<Schedule[]>("/schedules"),
          api<Settings>("/settings"),
          api<Draft[]>("/workflow-drafts"),
        ]);
        if (!alive) return;
        devices = d;
        workflows = w.sort((a, b) => b.version - a.version);
        drafts = savedDrafts;
        runs = r;
        reconcileRunSelection();
        schedules = s;
        settings = config;
        connected = true;
        loadError = "";
        if (!model.endpoint) model = { ...config.model };
        if (selectedRun) await loadRunEvents(selectedRun);
      } while (dirty && alive);
    } catch (e) {
      const message = e instanceof Error ? e.message : "连接失败";
      if (message !== loadError) notify.error(message);
      loadError = message;
      connected = false;
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
    if (working) return false;
    working = key;

    try {
      await fn();
      notify.success(message);
      await load();
      return true;
    } catch (e) {
      notify.error(e instanceof Error ? e.message : "操作失败");
      return false;
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
    if (message) notify.success(message);
  }
  async function markPoint(d: Device) {
    pointOrigin = tab;
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
        if (pointOrigin === "devices") {
          selectedDeviceId = deviceId;
          deviceDetailTab = "task";
        } else {
          workflowOpen = true;
          tab = "workflows";
        }
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
  async function startFlow(embedded = false) {
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
          version: embedded ? undefined : flow.version,
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
        if (embedded) deviceDetailTab = "task";
        else tab = "runs";
      },
      "任务已开始",
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
        notify.error(e instanceof Error ? e.message : "读取记录失败");
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
      if (cleared.size) notify.success(`已清理 ${cleared.size} 条记录`);
      if (failed) notify.error(`有 ${failed} 条未清理：${failure}`);
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
  function resumeOccupied(target: Run) {
    return activeRuns.some(
      (other) => other.id !== target.id && other.device_id === target.device_id,
    );
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
    const navigation = readHomeNavigation(page.url);
    tab = navigation.tab;
    selectedDeviceId = navigation.selectedDeviceId;
    deviceDetailTab = navigation.deviceDetailTab;
    workflowOpen = navigation.workflowOpen;
    selectedWorkflow = navigation.selectedWorkflow;
    selectedVersion = navigation.selectedVersion;
    selectedRun = navigation.selectedRun;
    navigationReady = true;
    theme =
      document.documentElement.dataset.theme === "dark" ? "dark" : "light";
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
        socket = new WebSocket(
          `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/ws`,
        );
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
      socket?.close();
      Object.values(pictures).forEach(URL.revokeObjectURL);
    };
  });
</script>

{#snippet taskLaunch(embedded: boolean)}
  {#if flow}
    <form
      class="task-launch"
      onsubmit={(event) => {
        event.preventDefault();
        void startFlow(embedded);
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
          先等待，再滑动；到达总时长自动结束。{embedded &&
          deviceRun &&
          busy(deviceRun)
            ? "以上设置用于下一次启动。"
            : "以上设置用于本次运行。"}
        </p>{/if}
      {#if embedded}
        <div class="card-actions device-task-start">
          <Button disabled={!!working || !!launchBlocked} type="submit"
            ><Play size={16} />直接运行</Button
          >
        </div>
      {:else}
        <div class="flow-controls">
          <label
            >版本<Dropdown
              items={workflows
                .filter((w) => w.id === flow.id)
                .map((w) => ({
                  value: String(w.version),
                  label: `第 ${w.version} 版`,
                }))}
              value={String(flow.version)}
              label="任务版本"
              onchange={(value) => (selectedVersion = Number(value))}
            /></label
          ><label
            >执行手机<Dropdown
              items={devices
                .filter(
                  (d) => !flow.devices.length || flow.devices.includes(d.id),
                )
                .map((d) => ({
                  value: d.id,
                  label: `${d.name} · ${d.online ? "在线" : "离线"}`,
                }))}
              bind:value={targetDevice}
              label="执行手机"
            /></label
          ><Button disabled={!!working || !!launchBlocked} type="submit"
            ><Play size={16} />运行此版本</Button
          >
        </div>
      {/if}
      {#if launchBlocked}<p class="hint" role="status">
          {launchBlocked}
        </p>{/if}
    </form>
  {/if}
{/snippet}

{#snippet runDetails(
  run: Run,
  events: Event[],
  deviceName: string,
  compact: boolean,
)}
  <article class="panel">
    <div class="panel-heading">
      <div>
        <h2>{run.workflow.name}</h2>
        <p>
          {deviceName} · 第 {run.workflow.version} 版
        </p>
      </div>
      <Badge variant={run.state === "needs_help" ? "destructive" : "default"}
        >{runLabel(run)}</Badge
      >
    </div>
    <div class="card-actions run-actions">
      {#if run.state === "paused" || run.state === "cancelled"}
        <Button
          disabled={!!working ||
            !connected ||
            resumeBlocked(run) ||
            resumeOccupied(run)}
          onclick={() =>
            action(
              "resume-run",
              () => api(`/runs/${run.id}/resume`, "POST"),
              "已请求继续，正在检查当前步骤",
            )}><Play size={16} aria-hidden="true" />继续任务</Button
        >
      {:else if busy(run)}
        <Button
          variant="outline"
          disabled={!!working || !connected || run.pause_requested}
          onclick={() =>
            action(
              "pause-run",
              () => api(`/runs/${run.id}/pause`, "POST"),
              "已请求暂停；显示“已暂停”后再操作手机",
            )}
        >
          {#if run.pause_requested}<LoaderCircle
              size={16}
              class="spin"
              aria-hidden="true"
            />正在暂停{:else}<Hand size={16} aria-hidden="true" />人工接管{/if}
        </Button>
      {/if}
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
        onclick={() => exportRun(run.id)}><Download size={17} />导出记录</Button
      >
      {#if !busy(run)}<Button
          variant="outline"
          disabled={!!working || !connected}
          onclick={() => requestClear(run)}
          ><Trash2 size={17} aria-hidden="true" />清理这条记录</Button
        >{/if}
    </div>
    {#if !compact || run.state !== "running" || run.error_detail}
      <div
        class="run-reason"
        class:run-paused={run.state === "paused"}
        aria-live="polite"
      >
        <RunFeedback message={run.reason} detail={run.error_detail} />
      </div>
    {/if}
    {#if run.state === "paused" || run.state === "cancelled"}<p
        class="run-resume-hint"
      >
        {resumeBlocked(run)
          ? "上次动作结果未确认，不能继续。请核对页面后重新开始，避免重复操作。"
          : resumeOccupied(run)
            ? "这台手机已有其他任务，请先停止该任务再继续。"
            : "保留原参数和已完成进度继续执行。请先确认手机在原应用的目标页面，再点击“继续任务”。"}
      </p>{/if}
    <div class="run-meta">
      <span>当前步骤：{stepTitle(run.workflow, run.node)}</span><span
        >模型：{run.model_calls} / {run.workflow.model_limit} 次</span
      >
    </div>
    {#if run.checkpoint.coin !== undefined}<p class="hint">
        金币：{String(run.checkpoint.coin ?? "待识别")} · 连续无增长：{String(
          run.checkpoint.idle ?? 0,
        )} 秒 · 已复查：{String(run.checkpoint.failed_swipes ?? 0)} 次
      </p>{:else if run.checkpoint.done !== undefined}<p class="hint">
        {run.workflow.nodes.some((node) =>
          ["fixed_tap_loop", "tap_loop"].includes(node.type),
        )
          ? "已点击"
          : "已尝试滑动"}
        {String(run.checkpoint.done)} 次
        {#if Number(run.checkpoint.uncertain_swipes || 0) || run.pending === "swipe"}（含
          {Number(run.checkpoint.uncertain_swipes || 0) +
            (run.pending === "swipe" ? 1 : 0)} 次结果未确认）{/if}
      </p>{/if}
    <h3 class="event-heading">判断与行动</h3>
    <ol class="event-list">
      {#each events as ev (ev.id)}<li>
          <time>{time(ev.at, true)}</time>
          <div>
            <small>步骤 {stepNumber(run.workflow, ev.node)}</small>
            <RunFeedback message={ev.message} />
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
  </article>
{/snippet}

{#snippet taskPanel(device: Device)}
  <details class="panel device-task-settings" bind:open={taskSettingsExpanded}>
    <summary class="device-task-settings-heading">
      <span>任务设置</span><ChevronDown
        size={16}
        class="device-task-settings-chevron"
        aria-hidden="true"
      />
    </summary>
    <div class="device-task-settings-content">
      <label
        >自动化任务
        <Dropdown
          items={taskChoices}
          bind:value={selectedWorkflow}
          label="选择自动化任务"
          placeholder="暂无可用任务"
          onchange={() => (selectedVersion = 0)}
        />
      </label>
      {#if deviceRun && busy(deviceRun)}<p class="hint">
          当前任务按原参数执行；下方设置用于下一次启动。
        </p>{/if}
      {@render taskLaunch(true)}
      {#if !flow}<p class="hint">请选择一个可用任务。</p>{/if}
    </div>
  </details>
  {#if deviceRun}
    {@render runDetails(
      deviceRun,
      deviceEventsId === deviceRun.id ? deviceEvents : [],
      device.name,
      true,
    )}
  {:else}
    <article class="panel device-task-empty">
      <History size={24} />
      <h2>暂无执行记录</h2>
      <p class="hint">设置任务并开始后，进度与执行记录会显示在这里。</p>
    </article>
  {/if}
{/snippet}

<svelte:head
  ><title>{pageTitle}</title><meta
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
            workflowOpen = false;
          }}
          aria-current={tab === item.id ? "page" : undefined}
          ><item.icon size={21} /><span>{item.label}</span
          >{#if item.id === "runs" && activeRuns.length}<span class="nav-count"
              >{activeRuns.length}</span
            >{/if}</button
        >{/each}
    </nav>
    <div class="sidebar-foot">
      <button
        type="button"
        class="theme-button"
        onclick={toggleTheme}
        aria-label={theme === "light" ? "切换夜间模式" : "切换日间模式"}
        title={theme === "light" ? "切换夜间模式" : "切换日间模式"}
      >
        {#if theme === "light"}<Moon size={16} aria-hidden="true" /><span
            class="theme-label">夜间模式</span
          >
        {:else}<Sun size={16} aria-hidden="true" /><span class="theme-label"
            >日间模式</span
          >{/if}
      </button>
    </div>
  </aside>
  <main>
    <section class="content" bind:this={contentElement}>
      {#if tab === "runs" || tab === "settings"}<div
          class="page-heading page-heading-simple"
        >
          <h1>{tabs.find((t) => t.id === tab)?.label}</h1>
          {#if tab === "runs" || activeRuns.length}<div class="page-actions">
              {#if tab === "runs"}<Button
                  variant="outline"
                  disabled={!!working || !connected || !finishedRuns.length}
                  onclick={() => requestClear()}
                  ><Trash2 size={17} aria-hidden="true" />清理已结束</Button
                >{/if}
              {#if activeRuns.length}<Button
                  variant="outline"
                  class="stop-button"
                  disabled={!!working}
                  onclick={stopAllRuns}><Square size={16} />全部停止</Button
                >{/if}
            </div>{/if}
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
          bind:selectedDeviceId
          onAdded={(device) => {
            devices = [
              ...devices.filter((item) => item.id !== device.id),
              device,
            ];
            selectedDeviceId = device.id;
            notify.success(`已连接 ${device.name}`);
            void load();
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
          bind:detailTab={deviceDetailTab}
          {taskPanel}
        />
      {:else if tab === "workflows"}
        <div hidden={workflowOpen && !!flow}>
          <TaskList
            {workflows}
            {drafts}
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
            onArchive={(workflow) =>
              action(
                `archive-${workflow.id}`,
                () =>
                  api(
                    `/workflows/${encodeURIComponent(workflow.id)}/archive`,
                    "PUT",
                  ),
                "任务已归档，相关定时已停用",
              )}
            onRestore={(workflow) =>
              action(
                `restore-${workflow.id}`,
                () =>
                  api(
                    `/workflows/${encodeURIComponent(workflow.id)}/archive`,
                    "DELETE",
                  ),
                "任务已恢复，定时仍保持停用",
              )}
          />
        </div>
        {#if workflowOpen && flow}
          <div class="page-heading page-heading-simple">
            <h1>任务详情</h1>
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
                variant={flow.archived ? "outline" : "default"}
                >{flow.archived ? "已归档" : "已保存"}</Badge
              >
            </div>
            <p class="hint">
              识别方式：{flow.recognition === "vision" ? "纯视觉" : "本地优先"} ·
              模型上限：{flow.model_limit} 次。{flow.recognition === "vision"
                ? "截图发送到模型服务；定位、复查、结果检查和重试均计入次数。"
                : "无需识别的动作直接执行。"}
            </p>
            {#if flow.archived}
              <p class="hint">
                任务已归档。恢复后可再次运行；原有定时需手动开启。
              </p>
            {:else}
              {@render taskLaunch(false)}
            {/if}
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
                    {#if nodeRun}<Badge>{runLabel(nodeRun)}</Badge>{/if}
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
        <ScheduleManager
          {schedules}
          {workflows}
          {devices}
          {loading}
          {connected}
          {working}
          activeCount={activeRuns.length}
          onStopAll={stopAllRuns}
          onSave={(id, input) =>
            action(
              "save-schedule",
              () =>
                api(
                  id ? `/schedules/${id}` : "/schedules",
                  id ? "PUT" : "POST",
                  input,
                ),
              "定时已保存",
            )}
          onToggle={(schedule) =>
            action(
              "toggle-schedule",
              () =>
                api(`/schedules/${schedule.id}`, "PUT", {
                  ...scheduleInput(schedule),
                  enabled: !schedule.enabled,
                }),
              schedule.enabled ? "定时已暂停" : "定时已开启",
            )}
          onDelete={(schedule) =>
            action(
              "delete-schedule",
              () => api(`/schedules/${schedule.id}`, "DELETE"),
              "定时已删除",
            )}
        />
      {:else if tab === "runs"}
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
                      : "outline"}>{runLabel(r)}</Badge
                  ></button
                >{/each}
            </div>
            {#if run}{@render runDetails(
                run,
                events,
                name(run.device_id),
                false,
              )}{/if}
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
                <dt>后台自启</dt>
                <dd>{settings?.autostart ? "已安装" : "尚未安装"}</dd>
              </div>
              <div>
                <dt>运行平台</dt>
                <dd>{settings?.platform || "未知"} · {settings?.arch || ""}</dd>
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
            <h2>识别模型</h2>
            <p class="hint">
              本地优先任务用模型辅助判断；纯视觉任务直接看图。本地文字识别：{settings?.local_ocr
                ? "可用"
                : "未安装"}。
            </p>
            <label>模型名称<input required bind:value={model.model} /></label
            ><label
              >接口地址<input
                type="url"
                required
                bind:value={model.endpoint}
              /></label
            ><label
              >接口格式<Dropdown
                label="接口格式"
                bind:value={model.api}
                items={[
                  { value: "responses", label: "Responses" },
                  { value: "chat_completions", label: "Chat Completions" },
                ]}
              /></label
            ><label>密钥变量<input required bind:value={model.key_env} /></label
            >
            <p class="hint">
              默认复用现有 MODEL_KEY。密钥内容不会显示在网页或记录中。
            </p>
            <Button type="submit" disabled={!!working || !connected}
              >保存模型设置</Button
            >
          </form>
          <VisionInspector {devices} {connected} />
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
