<script lang="ts">
  import { notify } from "$lib/notifications";
  import { tick } from "svelte";
  import {
    CalendarClock,
    Clock3,
    LoaderCircle,
    Pause,
    Pencil,
    Play,
    Plus,
    Search,
    Square,
    Trash2,
    X,
  } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import {
    launchMode,
    type Device,
    type Schedule,
    type Workflow,
    type Rule,
  } from "$lib/api";
  import {
    onceExpired,
    ruleLabel,
    scheduleTime,
    shanghaiDateInput,
    weekdays,
    type ScheduleInput,
  } from "$lib/schedules";

  let {
    schedules,
    workflows,
    devices,
    loading,
    connected,
    working,
    activeCount,
    onStopAll,
    onSave,
    onToggle,
    onDelete,
  }: {
    schedules: Schedule[];
    workflows: Workflow[];
    devices: Device[];
    loading: boolean;
    connected: boolean;
    working: string;
    activeCount: number;
    onStopAll: () => void;
    onSave: (id: string, input: ScheduleInput) => Promise<boolean>;
    onToggle: (schedule: Schedule) => Promise<boolean>;
    onDelete: (schedule: Schedule) => Promise<boolean>;
  } = $props();

  let search = $state("");
  let filter = $state("all");
  let editor = $state<HTMLDialogElement>();
  let deleteDialog = $state<HTMLDialogElement>();
  let nameInput = $state<HTMLInputElement>();
  let newButton = $state<HTMLElement | null>(null);
  let editingId = $state("");
  let deleting = $state<Schedule | null>(null);
  let scheduleName = $state("");
  let deviceId = $state("");
  let workflowId = $state("");
  let enabled = $state(true);
  let kind = $state<Rule["kind"]>("daily");
  let time = $state("09:00");
  let date = $state("");
  let minutes = $state(60);
  let days = $state<number[]>([1, 2, 3, 4, 5]);
  let tapCount = $state(10);
  let tapInterval = $state(5);
  let swipeMinutes = $state(15);
  let swipeInterval = $state(25);

  const availableWorkflows = $derived(workflows);
  const workflow = $derived(
    availableWorkflows.find((workflow) => workflow.id === workflowId),
  );
  const hasTap = $derived(
    workflow?.nodes.some((node) =>
      ["tap_loop", "fixed_tap_loop"].includes(node.type),
    ),
  );
  const compatibleDevices = $derived(
    devices.filter(
      (device) =>
        !workflow?.devices.length || workflow.devices.includes(device.id),
    ),
  );
  const selectedDevice = $derived(
    devices.find((device) => device.id === deviceId),
  );
  const blocked = $derived(
    !workflow
      ? "请先保存一个自动化任务。"
      : !compatibleDevices.some((device) => device.id === deviceId)
        ? "请选择适用于此任务的已登记手机。"
        : launchMode(workflow) === "device_point" && !selectedDevice?.tap_point
          ? "请先到设备详情标注这台手机的点击位置。"
          : kind === "weekly" && !days.length
            ? "请至少选择一天。"
            : "",
  );
  const visible = $derived(
    schedules
      .filter((schedule) => {
        const text = [
          schedule.name,
          deviceName(schedule.device_id),
          schedule.device_id,
          workflowName(schedule.workflow_id),
          ruleLabel(schedule.rule),
        ]
          .join(" ")
          .toLowerCase();
        return (
          text.includes(search.trim().toLowerCase()) &&
          (filter === "all" ||
            (filter === "enabled" ? schedule.enabled : !schedule.enabled))
        );
      })
      .sort(
        (a, b) =>
          Number(b.enabled) - Number(a.enabled) ||
          a.next_at.localeCompare(b.next_at),
      ),
  );

  function deviceName(id: string) {
    return devices.find((device) => device.id === id)?.name || id;
  }
  function workflowName(id: string) {
    return workflows.find((workflow) => workflow.id === id)?.name || id;
  }
  function chooseDevice() {
    if (!compatibleDevices.some((device) => device.id === deviceId))
      deviceId = compatibleDevices[0]?.id ?? "";
  }
  async function openEditor(schedule?: Schedule) {
    editingId = schedule?.id ?? "";
    scheduleName = schedule?.name ?? "";
    workflowId =
      schedule?.workflow_id ??
      (
        availableWorkflows.find((workflow) => workflow.id === "swipe") ||
        availableWorkflows[0]
      )?.id ??
      "";
    deviceId = schedule?.device_id ?? "";
    enabled = schedule?.enabled ?? true;
    kind = schedule?.rule.kind ?? "daily";
    time =
      schedule &&
      (schedule.rule.kind === "daily" || schedule.rule.kind === "weekly")
        ? schedule.rule.time
        : "09:00";
    date =
      schedule?.rule.kind === "once" ? shanghaiDateInput(schedule.rule.at) : "";
    minutes = schedule?.rule.kind === "interval" ? schedule.rule.minutes : 60;
    days =
      schedule?.rule.kind === "weekly"
        ? [...schedule.rule.days]
        : [1, 2, 3, 4, 5];
    tapCount = schedule?.tap?.count ?? 10;
    tapInterval = schedule?.tap?.interval ?? 5;
    swipeMinutes = schedule?.swipe?.minutes ?? 15;
    swipeInterval = schedule?.swipe?.interval ?? 25;

    await tick();
    if (!schedule) chooseDevice();
    editor?.showModal();
    nameInput?.focus();
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (working || !connected || blocked) return;
    let rule: Rule;
    if (kind === "once") {
      const at = new Date(`${date}:00+08:00`);
      if (!Number.isFinite(at.getTime()) || at.getTime() <= Date.now()) {
        notify.error("单次执行时间已过去，请选择未来的上海时间。");
        editor
          ?.querySelector<HTMLInputElement>('input[type="datetime-local"]')
          ?.focus();
        return;
      }
      rule = { kind: "once", at: at.toISOString() };
    } else if (kind === "interval")
      rule = { kind: "interval", minutes: Number(minutes) };
    else if (kind === "weekly")
      rule = { kind: "weekly", time, days: [...days].sort((a, b) => a - b) };
    else rule = { kind: "daily", time };

    const saved = await onSave(editingId, {
      name: scheduleName.trim(),
      device_id: deviceId,
      workflow_id: workflowId,
      enabled,
      rule,
      tap: hasTap
        ? { count: Number(tapCount), interval: Number(tapInterval) }
        : null,
      swipe:
        launchMode(workflow) === "swipe_settings"
          ? { minutes: Number(swipeMinutes), interval: Number(swipeInterval) }
          : null,
    });
    if (saved) editor?.close();
  }
  function requestDelete(schedule: Schedule) {
    deleting = schedule;

    deleteDialog?.showModal();
  }
  async function remove() {
    if (!deleting || working || !connected) return;
    const removed = await onDelete(deleting);
    if (removed) {
      deleteDialog?.close();
      deleting = null;
      newButton?.focus();
    }
  }
</script>

<header class="page-heading schedule-heading">
  <h1>定时任务</h1>
  <div class="schedule-toolbar" aria-label="搜索与筛选定时">
    <div class="schedule-search">
      <Search size={17} aria-hidden="true" />
      <label class="visually-hidden" for="schedule-search"
        >搜索定时名称、任务或手机</label
      >
      <input
        id="schedule-search"
        type="search"
        placeholder="搜索定时名称、任务或手机"
        bind:value={search}
      />
    </div>
    <div class="schedule-filter">
      <Dropdown
        label="筛选定时状态"
        bind:value={filter}
        items={[
          { value: "all", label: "全部定时" },
          { value: "enabled", label: "已开启" },
          { value: "disabled", label: "未开启" },
        ]}
      />
    </div>
    <Button
      bind:ref={newButton}
      disabled={loading ||
        !connected ||
        !!working ||
        !availableWorkflows.length ||
        !devices.length}
      onclick={() => void openEditor()}
      ><Plus size={16} aria-hidden="true" />新建定时</Button
    >
    {#if activeCount}
      <div class="schedule-toolbar-actions">
        <Button
          variant="outline"
          class="stop-button"
          disabled={!!working}
          onclick={onStopAll}
        >
          <Square size={14} aria-hidden="true" />全部停止
        </Button>
      </div>
    {/if}
  </div>
</header>

{#if loading}
  <div class="empty">
    <LoaderCircle class="spin" aria-hidden="true" />正在读取定时…
  </div>
{:else}
  {#if !visible.length}
    <div class="empty">
      <CalendarClock size={28} aria-hidden="true" />
      <h2>{schedules.length ? "没有符合条件的定时" : "还没有定时任务"}</h2>
      <p>
        {schedules.length
          ? "试试其他关键词或筛选条件。"
          : "新建定时，选择手机、任务和执行时间。"}
      </p>
      {#if search || filter !== "all"}<Button
          variant="outline"
          onclick={() => {
            search = "";
            filter = "all";
          }}>清除筛选</Button
        >{/if}
    </div>
  {:else}
    <div class="task-grid schedule-grid">
      {#each visible as schedule (schedule.id)}
        {@const expired = onceExpired(schedule)}
        <article class="task-card schedule-card" aria-label={schedule.name}>
          <div class="task-card-heading">
            <span class="task-icon" aria-hidden="true"
              ><CalendarClock size={17} /></span
            >
            <h2>{schedule.name}</h2>
            <span class="task-availability" class:ready={schedule.enabled}
              >{schedule.enabled
                ? "已开启"
                : expired
                  ? "已到期"
                  : "已暂停"}</span
            >
          </div>
          <p class="schedule-rule">{ruleLabel(schedule.rule)}</p>
          <div class="task-meta schedule-meta">
            <span>手机 <b>{deviceName(schedule.device_id)}</b></span>
            <span>任务 <b>{workflowName(schedule.workflow_id)}</b></span>
            {#if schedule.swipe}<span
                >时长 <b>{schedule.swipe.minutes} 分钟</b> · 间隔
                <b>{schedule.swipe.interval} 秒</b></span
              >{/if}
            {#if schedule.tap}<span
                >点击 <b>{schedule.tap.count} 次</b> · 间隔
                <b>{schedule.tap.interval} 秒</b></span
              >{/if}
          </div>
          <div class="schedule-next" class:muted={!schedule.enabled}>
            <Clock3 size={16} aria-hidden="true" />
            <div>
              <small>下次执行</small><strong
                >{schedule.enabled
                  ? scheduleTime(schedule.next_at)
                  : expired
                    ? "单次时间已过"
                    : "暂停中"}</strong
              >
            </div>
          </div>
          <p
            class="schedule-result"
            class:attention={schedule.last_result?.includes("跳过")}
          >
            <span>最近触发</span>{schedule.last_result || "暂无触发记录"}
          </p>
          <div class="task-card-footer">
            <span class="schedule-card-hint"
              >{expired
                ? "修改时间后可再次开启"
                : "使用执行时保存的任务内容"}</span
            >
            <div class="task-buttons">
              <Button
                variant="outline"
                disabled={!!working}
                onclick={() => void openEditor(schedule)}
                ><Pencil size={12} aria-hidden="true" />修改</Button
              >
              <Button
                variant="outline"
                disabled={!!working || !connected || expired}
                onclick={() => void onToggle(schedule)}
                >{#if schedule.enabled}<Pause
                    size={12}
                    aria-hidden="true"
                  />暂停{:else}<Play
                    size={12}
                    aria-hidden="true"
                  />开启{/if}</Button
              >
              <Button
                variant="ghost"
                class="stop-button"
                disabled={!!working || !connected}
                onclick={() => requestDelete(schedule)}
                ><Trash2 size={12} aria-hidden="true" />删除</Button
              >
            </div>
          </div>
        </article>
      {/each}
    </div>
  {/if}
{/if}

<dialog
  class="modal schedule-editor"
  bind:this={editor}
  aria-labelledby="schedule-editor-title"
  oncancel={(event) => {
    if (working) event.preventDefault();
  }}
>
  <form onsubmit={save}>
    <div class="schedule-dialog-heading">
      <div>
        <h2 id="schedule-editor-title">
          {editingId ? "修改定时" : "新建定时"}
        </h2>
        <p>选择任务与执行时间 · 上海时间</p>
      </div>
      <Button
        variant="ghost"
        aria-label="关闭定时表单"
        disabled={!!working}
        onclick={() => editor?.close()}
        ><X size={18} aria-hidden="true" /></Button
      >
    </div>
    <fieldset disabled={!!working} class="schedule-fields">
      <legend class="visually-hidden">定时内容</legend>
      <label
        >定时名称<input
          required
          maxlength="80"
          bind:this={nameInput}
          placeholder="例如：早上刷视频"
          bind:value={scheduleName}
        /></label
      >
      <div class="schedule-field-pair">
        <label
          >任务<Dropdown
            label="选择定时任务"
            bind:value={workflowId}
            placeholder={workflowId
              ? `${workflowName(workflowId)}（已删除）`
              : "暂无可用任务"}
            items={availableWorkflows.map((available) => ({
              value: available.id,
              label: available.name,
            }))}
            onchange={() => chooseDevice()}
          /></label
        >
        <label
          >手机<Dropdown
            label="选择定时手机"
            bind:value={deviceId}
            placeholder={deviceId ? deviceName(deviceId) : "暂无适用手机"}
            items={compatibleDevices.map((device) => ({
              value: device.id,
              label: `${device.name}${device.online ? "" : "（离线）"}`,
            }))}
          /></label
        >
      </div>
      {#if selectedDevice && (!selectedDevice.online || selectedDevice.locked)}<p
          class="hint"
        >
          可以保存未来定时；执行前请连接手机、亮屏并解锁。
        </p>{/if}
      <div class="schedule-field-pair">
        <label
          >执行规则<Dropdown
            label="选择执行规则"
            bind:value={kind}
            items={[
              { value: "daily", label: "每天" },
              { value: "weekly", label: "每周" },
              { value: "interval", label: "固定间隔" },
              { value: "once", label: "只执行一次" },
            ]}
          /></label
        >
        {#if kind === "once"}<label
            >日期与时间<input
              type="datetime-local"
              required
              bind:value={date}
            /></label
          >
        {:else if kind === "interval"}<label
            >间隔（分钟）<input
              type="number"
              min="1"
              max="525600"
              step="1"
              required
              bind:value={minutes}
            /></label
          >
        {:else}<label
            >时间<input type="time" required bind:value={time} /></label
          >{/if}
      </div>
      {#if kind === "weekly"}<fieldset class="schedule-days">
          <legend>星期（至少一天）</legend>
          <div class="weekdays">
            {#each weekdays as day, i}<label
                ><input
                  type="checkbox"
                  checked={days.includes(i + 1)}
                  onchange={(event) => {
                    days = event.currentTarget.checked
                      ? [...days, i + 1]
                      : days.filter((day) => day !== i + 1);
                  }}
                />{day}</label
              >{/each}
          </div>
        </fieldset>{/if}
      {#if kind === "interval"}<p class="hint">
          从本次保存时间起计时；修改或重新开启会重新计时。
        </p>{/if}
      {#if hasTap}<div class="schedule-field-pair">
          <label
            >点击次数<input
              type="number"
              min="1"
              max="10000"
              step="1"
              required
              bind:value={tapCount}
            /></label
          ><label
            >点击间隔（秒）<input
              type="number"
              min="1"
              max="86400"
              step="any"
              required
              bind:value={tapInterval}
            /></label
          >
        </div>{/if}
      {#if launchMode(workflow) === "swipe_settings"}<div
          class="schedule-field-pair"
        >
          <label
            >总时长（分钟）<input
              type="number"
              min="0.01"
              max="10080"
              step="any"
              required
              bind:value={swipeMinutes}
            /></label
          ><label
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
          保持手机当前页面；先等待，再滑动，到达时长结束。
        </p>{/if}
      <label class="awake"
        ><span>开启定时</span><input
          type="checkbox"
          class="switch"
          bind:checked={enabled}
        /></label
      >
    </fieldset>
    {#if blocked}<p class="schedule-validation" role="status">{blocked}</p>{/if}
    {#if !connected}<p class="schedule-validation" role="status">
        后台未连接，连接恢复后可保存。
      </p>{/if}
    <div class="schedule-dialog-actions">
      <Button
        variant="outline"
        disabled={!!working}
        onclick={() => editor?.close()}>取消</Button
      ><Button type="submit" disabled={!!working || !connected || !!blocked}
        >{#if working === "save-schedule"}<LoaderCircle
            size={16}
            class="spin"
            aria-hidden="true"
          />{:else}<CalendarClock size={16} aria-hidden="true" />{/if}{editingId
          ? "保存修改"
          : "保存定时"}</Button
      >
    </div>
  </form>
</dialog>

<dialog
  class="modal schedule-delete"
  bind:this={deleteDialog}
  aria-labelledby="schedule-delete-title"
  oncancel={(event) => {
    if (working) event.preventDefault();
  }}
>
  <h2 id="schedule-delete-title">删除这条定时？</h2>
  <p class="schedule-delete-name">{deleting?.name}</p>
  <p class="hint">删除后不再触发。已经开始的任务与执行记录保留。</p>
  <div class="schedule-dialog-actions">
    <Button
      variant="outline"
      disabled={!!working}
      onclick={() => deleteDialog?.close()}>取消</Button
    ><Button
      variant="destructive"
      disabled={!!working || !connected}
      onclick={() => void remove()}
      ><Trash2 size={16} aria-hidden="true" />确认删除</Button
    >
  </div>
</dialog>

<style>
  .schedule-heading {
    align-items: center;
    flex-direction: row;
  }
  .schedule-heading h1 {
    line-height: 1.3;
  }
  .schedule-toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    margin-left: auto;
  }
  .schedule-search {
    position: relative;
    width: 280px;
    min-width: 180px;
  }
  .schedule-search > :global(svg) {
    position: absolute;
    top: 50%;
    left: 13px;
    transform: translateY(-50%);
    pointer-events: none;
    color: var(--dim);
  }
  .schedule-search input {
    width: 100%;
    height: 40px;
    min-height: 40px;
    margin: 0;
    padding: 8px 12px 8px 39px;
    background: var(--panel);
    font-size: 13px;
  }
  .schedule-search input::placeholder {
    color: var(--dim);
  }
  .schedule-filter {
    width: 150px;
    flex: none;
  }
  .schedule-filter :global(.dropdown-trigger) {
    height: 40px;
    min-height: 40px;
  }
  .schedule-toolbar-actions {
    display: flex;
    justify-content: flex-end;
  }
  .schedule-heading :global([data-slot="button"]),
  .schedule-toolbar-actions :global([data-slot="button"]) {
    height: 40px;
    min-height: 40px;
    padding: 0 14px;
    font-size: 13px;
    line-height: 1;
    transition-property: background-color, color, border-color, box-shadow;
  }
  @media (max-width: 900px) {
    .schedule-heading {
      flex-wrap: wrap;
    }
    .schedule-toolbar {
      width: 100%;
      justify-content: flex-end;
    }
    .schedule-search {
      flex: 1;
      width: auto;
    }
  }
  @media (max-width: 660px) {
    .schedule-heading {
      flex-direction: row;
      align-items: center;
      flex-wrap: wrap;
      gap: 12px;
    }
    .schedule-toolbar {
      flex-wrap: wrap;
      gap: 10px;
    }
    .schedule-filter {
      width: 125px;
    }
    .schedule-heading :global([data-slot="button"]),
    .schedule-toolbar-actions :global([data-slot="button"]),
    .schedule-filter :global(.dropdown-trigger),
    .schedule-search input {
      height: 44px;
      min-height: 44px;
    }
  }
</style>
