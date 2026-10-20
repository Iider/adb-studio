<script lang="ts">
  import {
    ArrowLeft,
    Camera,
    ChevronRight,
    LoaderCircle,
    Pencil,
    Play,
    RefreshCw,
    Square,
    Sun,
    Moon,
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
  let deviceTab = $state("status");
  const selectedDevice = $derived(
    devices.find((device) => device.id === selectedDeviceId),
  );
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
  function openDevice(device: Device) {
    selectedDeviceId = device.id;
    deviceTab = "status";
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
  <div>
    <h1>{selectedDevice?.name || "设备管理"}</h1>
    <p class="intro">
      {selectedDevice
        ? `${selectedDevice.model} · ${selectedDevice.id}`
        : "管理 Android 设备、连接状态和设备详情。"}
    </p>
  </div>
  <div class="page-actions">
    {#if activeRuns.length}
      <Button
        variant="outline"
        class="stop-button"
        disabled={!!working}
        onclick={onStopAll}><Square size={14} />全部停止</Button
      >
    {/if}
    {#if selectedDevice}
      <Button variant="outline" onclick={() => (selectedDeviceId = "")}
        ><ArrowLeft size={14} />返回设备</Button
      >
      <Button variant="outline" onclick={() => onRename(selectedDevice)}
        ><Pencil size={14} />重命名</Button
      >
    {:else}
      <Button onclick={onRefresh} disabled={!!working || !connected}
        ><RefreshCw
          size={14}
          class={working === "refresh" ? "spin" : ""}
        />刷新设备</Button
      >
    {/if}
  </div>
</div>

{#if selectedDevice}
  {@const device = selectedDevice}
  {@const activeRun = activeRuns.find((run) => run.device_id === device.id)}
  <nav class="device-tabs" aria-label="设备详情">
    <button
      class:active={deviceTab === "status"}
      aria-pressed={deviceTab === "status"}
      onclick={() => (deviceTab = "status")}>设备状态</button
    >
    <button
      class:active={deviceTab === "screen"}
      aria-pressed={deviceTab === "screen"}
      onclick={() => (deviceTab = "screen")}>屏幕预览</button
    >
    <button
      disabled={!runs.some((run) => run.device_id === device.id)}
      onclick={() => onViewRuns(device)}>执行记录</button
    >
  </nav>
  <div class="device-detail-grid" class:screen-active={deviceTab === "screen"}>
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
              : "请先到执行记录暂停接管，再操作屏幕。"
            : "需要密码时，请在手机上解锁。"}
        </p>
        <div class="card-actions">
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
    <article class="panel device-screen">
      {#key device.id}
        <LiveScreen
          id={device.id}
          name={device.name}
          online={device.online}
          picture={pictures[device.id]}
          capturing={working === `photo-${device.id}`}
          onCapture={() => onCapture(device)}
        />
      {/key}
    </article>
  </div>
{:else}
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
        <article class="device-card">
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
            aria-label={`查看${device.name}设备详情`}
            onclick={() => openDevice(device)}>查看设备详情 →</Button
          >
        </article>
      {/each}
    </div>
  {/if}
{/if}
