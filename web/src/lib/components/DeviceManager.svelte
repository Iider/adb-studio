<script lang="ts">
  import {
    LayoutGrid,
    ChevronDown,
    X,
    Camera,
    ChevronRight,
    LoaderCircle,
    Pencil,
    Play,
    RefreshCw,
    Square,
    Sun,
    Moon,
    MonitorOff,
    Monitor,
  } from "@lucide/svelte";
  import LiveScreen from "$lib/components/LiveScreen.svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { busy, runLabel, time, type Device, type Run } from "$lib/api";

  let {
    devices,
    runs,
    pictures,
    working,
    loading,
    connected,
    lastUpdated,
    selectedDeviceId = $bindable(""),
    onRefresh,
    onCapture,
    onRename,
    onScreen,
    onAwake,
    onMarkPoint,
    onChooseTask,
    onViewRuns,
    onStopAll,
  }: {
    devices: Device[];
    runs: Run[];
    pictures: Record<string, string>;
    working: string;
    loading: boolean;
    connected: boolean;
    lastUpdated: string | null;
    selectedDeviceId?: string;
    onRefresh: () => void;
    onCapture: (device: Device) => void;
    onRename: (device: Device) => void;
    onScreen: (device: Device, action: "wake" | "lock") => void;
    onAwake: (device: Device, enabled: boolean) => void;
    onMarkPoint: (device: Device) => void;
    onChooseTask: (device: Device) => void;
    onViewRuns: (device: Device) => void;
    onStopAll: () => void;
  } = $props();

  let search = $state("");
  let deviceDialog = $state<HTMLDialogElement>();
  let screenDimensions = $state("");
  let screenControlled = $state(false);
  let screenOff = $state(false);
  let powerPending = $state(false);
  let liveScreen: LiveScreen | undefined = $state();
  const selectedDevice = $derived(
    devices.find((device) => device.id === selectedDeviceId),
  );
  $effect(() => {
    screenOff = selectedDevice?.display_off ?? false;
  });
  const activeRuns = $derived(runs.filter(busy));
  const filteredDevices = $derived(
    devices.filter((device) =>
      [device.name, device.model, device.id, ...device.endpoints]
        .join(" ")
        .toLowerCase()
        .includes(search.trim().toLowerCase()),
    ),
  );

  function awakeLabel(device: Device) {
    if (device.restore_pending) return "等待恢复";
    if (!device.keep_awake) return "已关闭";
    return device.awake_applied ? "已开启" : "等待手机";
  }
  $effect(() => {
    if (
      devices.length &&
      !devices.some((device) => device.id === selectedDeviceId)
    ) {
      selectedDeviceId = (devices.find((device) => device.online) || devices[0])
        .id;
    }
  });
  function openDevice(device: Device) {
    selectedDeviceId = device.id;
    deviceDialog?.close();
  }
  function showDevices() {
    search = "";
    deviceDialog?.showModal();
  }
</script>

{#snippet status(device: Device)}
  <span
    class="device-status"
    class:offline={!device.online}
    class:locked={device.online && device.locked}
  >
    <span class="status-dot" aria-hidden="true"></span>
    {!device.online ? "离线" : device.locked ? "需要解锁" : "在线"}
  </span>
{/snippet}

<div class="page-heading device-heading">
  <div class="device-selection">
    <h1 class="visually-hidden">设备管理</h1>
    <div class="device-selector-row">
      <label class="visually-hidden" for="device-picker">切换手机</label>
      <div class="device-picker-field">
        <select
          id="device-picker"
          class="device-picker"
          bind:value={selectedDeviceId}
          disabled={!devices.length}
        >
          {#if !devices.length}<option value=""
              >{loading ? "正在读取手机…" : "暂无手机"}</option
            >{/if}
          {#each devices as device (device.id)}<option value={device.id}
              >{device.name} · {device.online
                ? device.locked
                  ? "需要解锁"
                  : "在线"
                : "离线"}</option
            >{/each}
        </select>
        <ChevronDown class="device-picker-chevron" aria-hidden="true" />
      </div>
      <Button variant="outline" class="device-all-button" onclick={showDevices}
        ><LayoutGrid size={15} />查看全部</Button
      >
    </div>
    {#if !selectedDevice}<p class="intro">连接手机后，点击刷新设备。</p>{/if}
  </div>
  <div class="page-actions">
    {#if activeRuns.length}<Button
        variant="outline"
        class="stop-button"
        disabled={!!working}
        onclick={onStopAll}><Square size={14} />全部停止</Button
      >{/if}
    <Button
      variant="outline"
      onclick={onRefresh}
      disabled={!!working || !connected}
      ><RefreshCw
        size={14}
        class={working === "refresh" ? "spin" : ""}
      />刷新设备</Button
    >
    {#if selectedDevice}<Button
        variant="outline"
        onclick={() => onRename(selectedDevice)}
        ><Pencil size={14} />重命名</Button
      >{/if}
  </div>
</div>

{#if selectedDevice}
  {@const device = selectedDevice}
  {@const activeRun = activeRuns.find((run) => run.device_id === device.id)}
  <div class="device-detail-grid">
    <article class="panel device-screen">
      {#key device.id}
        <LiveScreen
          bind:this={liveScreen}
          id={device.id}
          name={device.name}
          online={device.online}
          picture={pictures[device.id]}
          capturing={working === `photo-${device.id}`}
          onCapture={() => onCapture(device)}
          bind:dimensions={screenDimensions}
          bind:controlled={screenControlled}
          bind:screenOff
          bind:powerPending
        />
      {/key}
    </article>
    <div class="device-stack">
      <div class="device-metrics">
        <article class="panel">
          <p>连接方式</p>
          <strong>{device.online ? device.connection : "离线"}</strong><small
            >{device.model}</small
          >
        </article>
        <article class="panel">
          <p>保持常亮</p>
          <strong>{awakeLabel(device)}</strong><small
            >{device.restore_pending
              ? "连接后恢复原设置"
              : "按手机分别保存"}</small
          >
        </article>
        <article class="panel">
          <p>运行状态</p>
          <strong>{activeRun ? runLabel(activeRun) : "空闲"}</strong><small
            >{activeRun ? activeRun.workflow.name : "当前没有任务"}</small
          >
        </article>
      </div>
      <article class="panel device-information">
        <h2>设备信息</h2>
        <dl>
          <div>
            <dt>手机型号</dt>
            <dd>{device.model}</dd>
          </div>
          <div>
            <dt>手机编号</dt>
            <dd>{device.id}</dd>
          </div>
          <div>
            <dt>连接地址</dt>
            <dd>{device.online ? device.endpoints.join("、") : "尚未连接"}</dd>
          </div>
          <div>
            <dt>最近在线</dt>
            <dd>{time(device.last_seen)}</dd>
          </div>
          <div>
            <dt>点击位置</dt>
            <dd>
              {device.tap_point
                ? `${device.tap_point.x}，${device.tap_point.y}`
                : "尚未标注"}
            </dd>
          </div>
          <div>
            <dt>投屏尺寸</dt>
            <dd>{screenDimensions || "未投屏"}</dd>
          </div>
          <div>
            <dt>操作模式</dt>
            <dd>
              {screenDimensions
                ? screenControlled
                  ? "操作手机"
                  : "仅观看"
                : "未投屏"}
            </dd>
          </div>
        </dl>
        <label class="awake">
          <span
            >保持常亮<small
              >{device.restore_pending
                ? "等待恢复原设置"
                : device.keep_awake
                  ? "建议保持接电"
                  : "按原来的息屏设置"}</small
            ></span
          >
          <input
            class="switch"
            type="checkbox"
            checked={device.keep_awake}
            disabled={!!working}
            onchange={(event) => onAwake(device, event.currentTarget.checked)}
          />
        </label>
      </article>
      <article class="panel device-quick-actions">
        <h2>快捷操作</h2>
        <p class="hint">
          {activeRun
            ? activeRun.state === "paused"
              ? "任务已暂停，可以手动操作；完成后到执行记录继续。"
              : "开启投屏或关闭实体屏幕时，会先暂停当前任务。"
            : "需要密码时，请在手机上解锁。"}
        </p>
        <p class="hint">关闭实体屏幕后仍可投屏操作；未投屏时会自动连接。</p>
        <div class="card-actions">
          <Button
            variant="outline"
            disabled={!device.online || powerPending || !!working}
            onclick={() => liveScreen?.setDisplayPower(screenOff)}
          >
            {#if powerPending}<LoaderCircle size={15} class="spin" />
            {:else if screenOff}<Monitor size={15} />
            {:else}<MonitorOff size={15} />{/if}
            {powerPending
              ? "正在切换…"
              : screenOff
                ? "点亮手机屏幕"
                : "关闭手机屏幕"}
          </Button>
          <Button
            variant="outline"
            disabled={!device.online ||
              !!working ||
              (!!activeRun && activeRun.state !== "paused")}
            onclick={() => onScreen(device, "wake")}
            ><Sun size={15} />唤醒／尝试解锁</Button
          >
          <Button
            variant="outline"
            disabled={!device.online ||
              !!working ||
              (!!activeRun && activeRun.state !== "paused")}
            onclick={() => onScreen(device, "lock")}
            ><Moon size={15} />锁屏</Button
          >
          <Button
            variant="outline"
            disabled={!device.online || !!working}
            onclick={() => onCapture(device)}><Camera size={15} />截图</Button
          >
          <Button
            variant="outline"
            disabled={!device.online || !!working}
            onclick={() => onMarkPoint(device)}>标注点击位置</Button
          >
          <Button
            disabled={!device.online || !!working || !!activeRun}
            onclick={() => onChooseTask(device)}
            ><Play size={15} />选择任务</Button
          >
        </div>
        {#if activeRun}
          <button class="running-link" onclick={() => onViewRuns(device)}
            >{runLabel(activeRun)} · 查看进度<ChevronRight size={14} /></button
          >
        {/if}
      </article>
    </div>
  </div>
{:else}
  <div class="empty">
    {#if loading}<LoaderCircle
        class="spin"
      />正在读取手机…{:else}尚未发现手机，请连接后刷新。{/if}
  </div>
{/if}

<dialog
  bind:this={deviceDialog}
  class="modal device-directory"
  aria-labelledby="device-directory-title"
>
  <div class="device-directory-heading">
    <div>
      <h2 id="device-directory-title">全部手机</h2>
      <p class="hint">选择一台手机，切换到对应设备。</p>
    </div>
    <Button
      variant="outline"
      aria-label="关闭设备列表"
      onclick={() => deviceDialog?.close()}><X size={18} /></Button
    >
  </div>
  <div class="device-tools">
    <label class="visually-hidden" for="device-search"
      >搜索设备名称、型号或序列号</label
    >
    <input
      id="device-search"
      class="device-search"
      type="search"
      bind:value={search}
      placeholder="搜索设备名称、型号或序列号"
    />
    <span
      title={lastUpdated ? `更新于 ${time(lastUpdated, true)}` : "等待后台连接"}
      >共 {devices.length} 台设备 · {devices.filter((device) => device.online)
        .length} 台在线</span
    >
  </div>
  {#if loading}
    <div class="empty"><LoaderCircle class="spin" />正在读取手机…</div>
  {:else if !filteredDevices.length}
    <div class="empty">
      {search ? "未找到匹配的设备" : "尚未发现手机，请连接后刷新。"}
    </div>
  {:else}
    <div class="device-grid">
      {#each filteredDevices as device (device.id)}
        <article
          class="device-card"
          class:selected={device.id === selectedDeviceId}
        >
          <div class="device-row">
            <div class="phone-icon" aria-hidden="true"></div>
            {@render status(device)}
          </div>
          <h2>{device.name}</h2>
          <p class="model">{device.model}</p>
          <div class="device-rule"></div>
          <dl class="device-specs">
            <div>
              <dt>连接方式</dt>
              <dd>{device.connection}</dd>
            </div>
            <div>
              <dt>设备地址</dt>
              <dd
                title={device.online ? device.endpoints.join("、") : "尚未连接"}
              >
                {device.online
                  ? device.connection === "USB"
                    ? "USB"
                    : device.endpoints[0]
                  : "—"}
              </dd>
            </div>
            <div>
              <dt>保持常亮</dt>
              <dd>{awakeLabel(device)}</dd>
            </div>
          </dl>
          <div class="device-bar" aria-hidden="true">
            <span class:enabled={device.keep_awake && device.awake_applied}
            ></span>
          </div>
          <Button
            variant="outline"
            class="device-open"
            aria-label={`切换到${device.name}`}
            onclick={() => openDevice(device)}
            >{device.id === selectedDeviceId ? "当前设备" : "切换到此手机"} →</Button
          >
        </article>
      {/each}
    </div>
  {/if}
</dialog>
