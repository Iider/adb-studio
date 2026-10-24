<script lang="ts">
  import type { Snippet } from "svelte";
  import {
    X,
    Camera,
    LoaderCircle,
    Pencil,
    RefreshCw,
    Sun,
    Moon,
    MonitorOff,
    Monitor,
    Wifi,
  } from "@lucide/svelte";
  import LiveScreen from "$lib/components/LiveScreen.svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import {
    api,
    busy,
    runLabel,
    time,
    type Device,
    type NearbyDevice,
    type Run,
  } from "$lib/api";

  let {
    devices,
    runs,
    pictures,
    working,
    loading,
    connected,
    selectedDeviceId = $bindable(""),
    onCapture,
    onRename,
    onScreen,
    onAwake,
    taskPanel,
    detailTab = $bindable("parameters"),
    onAdded,
  }: {
    devices: Device[];
    runs: Run[];
    pictures: Record<string, string>;
    working: string;
    loading: boolean;
    connected: boolean;
    selectedDeviceId?: string;
    onCapture: (device: Device) => void;
    onRename: (device: Device) => void;
    onScreen: (device: Device, action: "wake" | "lock") => void;
    onAwake: (device: Device, enabled: boolean) => void;
    taskPanel: Snippet<[Device]>;
    detailTab?: "parameters" | "task";
    onAdded: (device: Device) => void;
  } = $props();

  let lanDialog = $state<HTMLDialogElement>();
  let pairDetails = $state<HTMLDetailsElement>();
  let address = $state("");
  let pairAddress = $state("");
  let pairCode = $state("");
  let lanPending = $state(false);
  let lanError = $state("");
  let paired = $state(false);
  let lanScanning = $state(false);
  let scanAttempted = $state(false);
  let nearbyDevices = $state<NearbyDevice[]>([]);
  let screenDimensions = $state("");
  let screenControlled = $state(false);
  let screenOff = $state(false);
  let powerPending = $state(false);
  let liveScreen: LiveScreen | undefined = $state();
  let sidePanel = $state<HTMLDivElement>();
  $effect(() => {
    detailTab;
    selectedDeviceId;
    sidePanel?.scrollTo(0, 0);
  });
  const selectedDevice = $derived(
    devices.find((device) => device.id === selectedDeviceId),
  );
  const deviceItems = $derived(
    devices.map((device) => ({
      value: device.id,
      label: device.name,
      description: device.model,
      device,
    })),
  );
  $effect(() => {
    screenOff = selectedDevice?.display_off ?? false;
  });
  const activeRuns = $derived(runs.filter(busy));

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
  async function scanLan() {
    if (lanScanning || lanPending) return;
    lanScanning = true;
    lanError = "";

    scanAttempted = false;
    try {
      nearbyDevices = await api<NearbyDevice[]>("/devices/discover");
      scanAttempted = true;
    } catch (error) {
      lanError = error instanceof Error ? error.message : "扫描失败";
    } finally {
      lanScanning = false;
    }
  }
  function chooseNearby(device: NearbyDevice) {
    if (device.kind === "connect") {
      address = device.address;
    } else {
      pairAddress = device.address;
      if (pairDetails) pairDetails.open = true;
    }
  }
  async function pairLan(event: SubmitEvent) {
    paired = false;
    event.preventDefault();
    if (lanPending) return;
    lanPending = true;
    lanError = "";

    try {
      await api("/devices/pair", "POST", {
        address: pairAddress.trim(),
        code: pairCode.trim(),
      });
      paired = true;
      pairCode = "";
    } catch (error) {
      lanError = error instanceof Error ? error.message : "配对失败";
    } finally {
      lanPending = false;
    }
  }
  async function connectLan(event: SubmitEvent) {
    event.preventDefault();
    if (lanPending) return;
    lanPending = true;
    lanError = "";

    try {
      const device = await api<Device>("/devices/connect", "POST", {
        address: address.trim(),
      });
      onAdded(device);
      lanDialog?.close();
    } catch (error) {
      lanError = error instanceof Error ? error.message : "连接失败";
    } finally {
      lanPending = false;
    }
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
      <div class="device-picker-field">
        <Dropdown
          items={deviceItems}
          bind:value={selectedDeviceId}
          label="切换手机"
          placeholder={loading ? "正在读取手机…" : "暂无手机"}
        >
          {#snippet leading(close)}
            <Button
              variant="ghost"
              class="device-picker-add"
              role="menuitem"
              tabindex={-1}
              disabled={!connected}
              onclick={() => {
                close();
                lanError = "";
                paired = false;
                nearbyDevices = [];
                scanAttempted = false;
                lanDialog?.showModal();
              }}><Wifi size={15} />添加局域网设备</Button
            >
          {/snippet}
          {#snippet selection(item)}
            {item.label} · {item.device.online
              ? item.device.locked
                ? "需要解锁"
                : "在线"
              : "离线"}
          {/snippet}
          {#snippet content(item)}
            <span class="device-picker-option">
              <span class="device-picker-name">
                <strong>{item.label}</strong>
                <small>{item.description}</small>
              </span>
              {@render status(item.device)}
            </span>
          {/snippet}
          {#snippet actions(item, close)}
            <Button
              variant="ghost"
              size="icon"
              class="device-picker-edit"
              role="menuitem"
              tabindex={-1}
              aria-label={`修改${item.label}名称`}
              title={`修改${item.label}名称`}
              disabled={!!working}
              onclick={() => {
                close();
                onRename(item.device);
              }}><Pencil size={15} aria-hidden="true" /></Button
            >
          {/snippet}
        </Dropdown>
      </div>
      {#if selectedDevice}
        {@const device = selectedDevice}
        {@const activeRun = activeRuns.find(
          (run) => run.device_id === device.id,
        )}
        <div class="device-actions" role="group" aria-label="手机操作">
          <Button
            variant="outline"
            disabled={!device.online || powerPending || !!working}
            title="关闭实体屏幕后仍可投屏操作；未投屏时会自动连接。"
            onclick={() => liveScreen?.setDisplayPower(screenOff)}
          >
            {#if powerPending}<LoaderCircle size={15} class="spin" />
            {:else if screenOff}<Monitor size={15} />
            {:else}<MonitorOff size={15} />{/if}
            {powerPending
              ? "正在切换…"
              : screenOff
                ? "点亮屏幕"
                : "关闭手机屏幕"}
          </Button>
          <Button
            variant="outline"
            disabled={!device.online ||
              !!working ||
              (!!activeRun && activeRun.state !== "paused")}
            title="唤醒并尝试解锁；需要密码时，请在手机上解锁。"
            onclick={() => onScreen(device, "wake")}
            ><Sun size={15} />唤醒</Button
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
        </div>
      {/if}
    </div>
    {#if !selectedDevice}<p class="intro">
        连接 USB 手机，或从下拉列表添加局域网设备。
      </p>{/if}
  </div>
  <div class="device-detail-tabs" role="tablist" aria-label="设备详情">
    {#each [{ id: "parameters", label: "设备参数" }, { id: "task", label: "任务详情" }] as item}
      <button
        type="button"
        role="tab"
        id={`device-tab-${item.id}`}
        aria-selected={detailTab === item.id}
        aria-controls="device-side-panel"
        tabindex={detailTab === item.id ? 0 : -1}
        disabled={!selectedDevice}
        onclick={() => (detailTab = item.id as typeof detailTab)}
        onkeydown={(event) => {
          if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
            return;
          event.preventDefault();
          detailTab =
            event.key === "Home"
              ? "parameters"
              : event.key === "End"
                ? "task"
                : detailTab === "task"
                  ? "parameters"
                  : "task";
          document.getElementById(`device-tab-${detailTab}`)?.focus();
        }}>{item.label}</button
      >
    {/each}
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
          bind:dimensions={screenDimensions}
          bind:controlled={screenControlled}
          bind:screenOff
          bind:powerPending
        />
      {/key}
    </article>
    <div
      bind:this={sidePanel}
      class="device-stack device-side-panel"
      id="device-side-panel"
      role="tabpanel"
      aria-labelledby={`device-tab-${detailTab}`}
      tabindex="0"
    >
      {#if detailTab === "parameters"}
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
              <dd>
                {device.online ? device.endpoints.join("、") : "尚未连接"}
              </dd>
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
      {:else}
        {@render taskPanel(device)}
      {/if}
    </div>
  </div>
{:else}
  <div class="empty">
    {#if loading}<LoaderCircle
        class="spin"
      />正在读取手机…{:else}尚未发现手机，请连接 USB 手机或添加局域网设备。{/if}
  </div>
{/if}

<dialog
  bind:this={lanDialog}
  class="modal lan-device-modal"
  aria-labelledby="lan-device-title"
>
  <div class="lan-device-heading">
    <div>
      <h2 id="lan-device-title">添加局域网设备</h2>
      <p class="hint">
        手机与运行工作台的电脑或 NAS 接入同一局域网，并开启无线调试。
      </p>
    </div>
    <Button
      variant="outline"
      aria-label="关闭添加设备"
      disabled={lanPending}
      onclick={() => lanDialog?.close()}><X size={18} /></Button
    >
  </div>
  <div class="lan-scan">
    <Button
      variant="outline"
      disabled={lanScanning || lanPending}
      onclick={() => void scanLan()}
      ><RefreshCw size={14} class={lanScanning ? "spin" : ""} />{lanScanning
        ? "正在扫描…"
        : "扫描局域网设备"}</Button
    >
    <p class="hint">仅显示已开启无线调试、正在广播的手机。</p>
  </div>
  {#if scanAttempted}
    {#if nearbyDevices.length}
      <ul class="lan-results">
        {#each nearbyDevices as device (`${device.kind}-${device.address}`)}
          <li>
            <button type="button" onclick={() => chooseNearby(device)}>
              <span>
                <strong
                  >{device.kind === "connect" ? "连接地址" : "配对地址"}</strong
                >
                <small>{device.name}</small>
              </span>
              <span class="lan-result-address">{device.address}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="hint lan-empty" role="status">
        未发现设备。请保持手机无线调试开启，或在下方手动填写地址。
      </p>
    {/if}
  {/if}
  <form class="lan-device-form" onsubmit={connectLan}>
    <label for="lan-address">连接地址</label>
    <input
      id="lan-address"
      bind:value={address}
      placeholder="例如 192.168.1.20:37123"
      autocomplete="off"
      spellcheck="false"
      required
    />
    <p class="hint">填写手机“无线调试”页面显示的 IP 地址和连接端口。</p>
    <Button type="submit" disabled={lanPending || !address.trim()}
      >{lanPending ? "正在连接…" : "连接手机"}</Button
    >
  </form>
  <details bind:this={pairDetails} class="lan-pair">
    <summary>首次连接？先配对</summary>
    <p class="hint">
      在手机上打开“使用配对码配对设备”，填写弹窗中的地址与 6
      位配对码。配对端口与上面的连接端口不同。
    </p>
    <form class="lan-device-form" onsubmit={pairLan}>
      <label for="lan-pair-address">配对地址</label>
      <input
        id="lan-pair-address"
        bind:value={pairAddress}
        placeholder="例如 192.168.1.20:41234"
        autocomplete="off"
        spellcheck="false"
        required
      />
      <label for="lan-pair-code">6 位配对码</label>
      <input
        id="lan-pair-code"
        bind:value={pairCode}
        inputmode="numeric"
        pattern={"[0-9]{6}"}
        maxlength="6"
        autocomplete="one-time-code"
        required
      />
      <Button type="submit" variant="outline" disabled={lanPending}
        >{lanPending ? "正在配对…" : "配对手机"}</Button
      >
    </form>
  </details>
  {#if paired}<p class="lan-message" role="status">
      配对成功。现在填写连接地址并连接手机。
    </p>{/if}
  {#if lanError}<p class="lan-error" role="alert">{lanError}</p>{/if}
</dialog>
