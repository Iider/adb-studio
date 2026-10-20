<script lang="ts">
  import { tick } from "svelte";
  import {
    CalendarClock,
    Clock3,
    LoaderCircle,
    Pause,
    Pencil,
    Play,
    Plus,
    Square,
    Trash2,
    X,
  } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import type { Device, Schedule, Workflow, Rule } from "$lib/api";
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
    error,
    activeCount,
    onStopAll,
    onSave,
    onToggle,
    onDelete,
    onViewTasks,
  }: {
    schedules: Schedule[];
    workflows: Workflow[];
    devices: Device[];
    loading: boolean;
    connected: boolean;
    working: string;
    error: string;
    activeCount: number;
    onStopAll: () => void;
    onSave: (id: string, input: ScheduleInput) => Promise<boolean>;
    onToggle: (schedule: Schedule) => Promise<boolean>;
    onDelete: (schedule: Schedule) => Promise<boolean>;
    onViewTasks: () => void;
  } = $props();

  let search = $state("");
  let filter = $state("all");
  let editor = $state<HTMLDialogElement>();
  let deleteDialog = $state<HTMLDialogElement>();
  let nameInput = $state<HTMLInputElement>();
  let editorError = $state<HTMLParagraphElement>();
  let deleteError = $state<HTMLParagraphElement>();
  let newButton = $state<HTMLElement | null>(null);
  let editingId = $state("");
  let deleting = $state<Schedule | null>(null);
  let formError = $state("");
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

  const activeWorkflows = $derived(
    workflows.filter((workflow) => workflow.active),
  );
  const workflow = $derived(
    activeWorkflows.find((workflow) => workflow.id === workflowId),
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
      ? "请先到自动化任务页试跑并启用此任务。"
      : !compatibleDevices.some((device) => device.id === deviceId)
        ? "请选择适用于此任务的已登记手机。"
        : workflowId === "point-click" && !selectedDevice?.tap_point
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
        activeWorkflows.find((workflow) => workflow.id === "swipe") ||
        activeWorkflows[0]
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
    formError = "";
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
        formError = "单次执行时间已过去，请选择未来的上海时间。";
        await tick();
        editorError?.focus();
        return;
      }
      rule = { kind: "once", at: at.toISOString() };
    } else if (kind === "interval")
      rule = { kind: "interval", minutes: Number(minutes) };
    else if (kind === "weekly")
      rule = { kind: "weekly", time, days: [...days].sort((a, b) => a - b) };
    else rule = { kind: "daily", time };
    formError = "";
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
        workflowId === "swipe"
          ? { minutes: Number(swipeMinutes), interval: Number(swipeInterval) }
          : null,
    });
    if (saved) editor?.close();
    else {
      formError = error || "保存失败，请重试。";
      await tick();
      editorError?.focus();
    }
  }
  function requestDelete(schedule: Schedule) {
    deleting = schedule;
    formError = "";
    deleteDialog?.showModal();
  }
  async function remove() {
    if (!deleting || working || !connected) return;
    const removed = await onDelete(deleting);
    if (removed) {
      deleteDialog?.close();
      deleting = null;
      newButton?.focus();
    } else {
      formError = error || "删除失败，请重试。";
      await tick();
      deleteError?.focus();
    }
  }
</script>

<div class="page-heading task-heading">
  <div>
    <h1>定时任务</h1>
    <p class="intro">
      共 {schedules.length} 条 · {schedules.filter(
        (schedule) => schedule.enabled,
      ).length} 条已开启 · 上海时间
    </p>
  </div>
  <div class="page-actions task-tools schedule-tools">
    <label class="visually-hidden" for="schedule-search"
      >搜索定时名称、任务或手机</label
    >
    <input
      id="schedule-search"
      class="task-search"
      type="search"
      placeholder="搜索定时名称、任务或手机"
      bind:value={search}
    />
    <label class="visually-hidden" for="schedule-filter">筛选定时状态</label>
    <select id="schedule-filter" class="task-filter" bind:value={filter}>
      <option value="all">全部定时</option><option value="enabled"
        >已开启</option
      ><option value="disabled">未开启</option>
    </select>
    <Button
      bind:ref={newButton}
      disabled={loading ||
        !connected ||
        !!working ||
        !activeWorkflows.length ||
        !devices.length}
      onclick={() => void openEditor()}
      ><Plus size={14} aria-hidden="true" />新建定时</Button
    >
    {#if activeCount}<Button
        variant="outline"
        class="stop-button"
        disabled={!!working}
        onclick={onStopAll}
        ><Square size={14} aria-hidden="true" />全部停止</Button
      >{/if}
  </div>
</div>

{#if loading}
  <div class="empty">
    <LoaderCircle class="spin" aria-hidden="true" />正在读取定时…
  </div>
{:else}
  {#if !activeWorkflows.length || !devices.length}
    <div class="schedule-note">
      <CalendarClock size={18} aria-hidden="true" />
      <div>
        {!devices.length
          ? "请先在设备管理中确认已登记的手机。"
          : "任务试跑并启用后，即可新建定时。"}
      </div>
      {#if !activeWorkflows.length}<Button
          variant="outline"
          onclick={onViewTasks}>查看任务</Button
        >{/if}
    </div>
  {/if}
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
                : "使用届时已启用的任务版本"}</span
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
  <p class="schedule-footnote">
    离线、忙碌或后台停机错过的定时点会跳过；已开始的任务掉线后等待手机。
  </p>
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
          >任务<select
            required
            bind:value={workflowId}
            onchange={(event) => {
              workflowId = event.currentTarget.value;
              chooseDevice();
            }}
          >
            {#if !workflow}<option value={workflowId} disabled
                >{workflowId
                  ? `${workflowName(workflowId)}（未启用）`
                  : "暂无已启用任务"}</option
              >{/if}
            {#each activeWorkflows as available}<option value={available.id}
                >{available.name}</option
              >{/each}
          </select></label
        >
        <label
          >手机<select required bind:value={deviceId}>
            {#if !compatibleDevices.some((device) => device.id === deviceId)}<option
                value={deviceId}
                disabled
                >{deviceId ? deviceName(deviceId) : "暂无适用手机"}</option
              >{/if}
            {#each compatibleDevices as device}<option value={device.id}
                >{device.name}{device.online ? "" : "（离线）"}</option
              >{/each}
          </select></label
        >
      </div>
      {#if selectedDevice && (!selectedDevice.online || selectedDevice.locked)}<p
          class="hint"
        >
          可以保存未来定时；执行前请连接手机、亮屏并解锁。
        </p>{/if}
      <div class="schedule-field-pair">
        <label
          >执行规则<select bind:value={kind}
            ><option value="daily">每天</option><option value="weekly"
              >每周</option
            ><option value="interval">固定间隔</option><option value="once"
              >只执行一次</option
            ></select
          ></label
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
      {#if workflowId === "swipe"}<div class="schedule-field-pair">
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
    {#if formError}<p
        class="schedule-validation"
        role="alert"
        tabindex="-1"
        bind:this={editorError}
      >
        {formError}
      </p>{/if}
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
  {#if formError}<p
      class="schedule-validation"
      role="alert"
      tabindex="-1"
      bind:this={deleteError}
    >
      {formError}
    </p>{/if}
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
