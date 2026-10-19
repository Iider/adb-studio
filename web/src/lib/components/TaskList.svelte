<script lang="ts">
  import {
    Hand,
    History,
    LoaderCircle,
    MousePointer2,
    Play,
    RefreshCw,
    Square,
    Workflow as FlowIcon,
  } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import {
    state as runState,
    time,
    type Device,
    type Run,
    type Schedule,
    type Workflow,
  } from "$lib/api";
  import { summarizeTasks } from "$lib/tasks";

  let {
    workflows,
    devices,
    runs,
    schedules,
    loading,
    connected,
    working,
    onOpen,
    onViewRuns,
    onStop,
    onStopAll,
    onRefresh,
  }: {
    workflows: Workflow[];
    devices: Device[];
    runs: Run[];
    schedules: Schedule[];
    loading: boolean;
    connected: boolean;
    working: string;
    onOpen: (workflow: Workflow) => void;
    onViewRuns: (run: Run) => void;
    onStop: (run: Run) => void;
    onStopAll: () => void;
    onRefresh: () => void;
  } = $props();

  let search = $state("");
  let filter = $state("all");
  const tasks = $derived(summarizeTasks(workflows, runs, schedules));
  const visibleTasks = $derived(
    tasks.filter((task) => {
      const workflow = task.workflow;
      const text = [
        workflow.name,
        workflow.id,
        workflow.description,
        ...workflow.devices,
        ...devices
          .filter(
            (device) =>
              !workflow.devices.length || workflow.devices.includes(device.id),
          )
          .flatMap((device) => [device.name, device.model, device.id]),
      ]
        .join(" ")
        .toLowerCase();
      return (
        text.includes(search.trim().toLowerCase()) &&
        (filter === "all" ||
          (filter === "ready" && !!task.active) ||
          (filter === "trial" &&
            (!task.active || task.latest.version > workflow.version)) ||
          (filter === "running" && task.inProgress.length > 0))
      );
    }),
  );

  function deviceName(id: string) {
    return devices.find((device) => device.id === id)?.name || id;
  }

  function taskIcon(workflow: Workflow) {
    if (workflow.nodes.some((node) => node.type === "swipe_loop")) return Hand;
    if (
      workflow.nodes.some((node) =>
        ["tap_loop", "fixed_tap_loop"].includes(node.type),
      )
    )
      return MousePointer2;
    return FlowIcon;
  }
</script>

<div class="page-heading task-heading">
  <div>
    <h1>自动化任务</h1>
    <p class="intro">
      共 {tasks.length} 项任务 · {tasks.filter((task) => task.active).length} 项已启用
      · {tasks.filter((task) => task.inProgress.length).length} 项进行中
    </p>
  </div>
  <div class="page-actions task-tools">
    <label class="visually-hidden" for="task-search">搜索任务名称或手机</label>
    <input
      id="task-search"
      class="task-search"
      type="search"
      placeholder="搜索任务名称或手机"
      bind:value={search}
    />
    <label class="visually-hidden" for="task-filter">筛选任务状态</label>
    <select id="task-filter" class="task-filter" bind:value={filter}>
      <option value="all">全部任务</option>
      <option value="ready">已启用</option>
      <option value="trial">待试跑</option>
      <option value="running">进行中</option>
    </select>
    <Button disabled={!connected || !!working} onclick={onRefresh}
      ><RefreshCw
        size={14}
        class={working === "refresh-tasks" ? "spin" : ""}
      />刷新任务</Button
    >
    {#if tasks.some((task) => task.inProgress.length)}<Button
        variant="outline"
        class="stop-button"
        disabled={!!working}
        onclick={onStopAll}><Square size={14} />全部停止</Button
      >{/if}
  </div>
</div>

{#if loading}
  <div class="empty"><LoaderCircle class="spin" />正在读取任务…</div>
{:else if !visibleTasks.length}
  <div class="empty">
    <FlowIcon size={28} />
    <h2>{tasks.length ? "没有符合条件的任务" : "尚未添加任务"}</h2>
    <p>
      {tasks.length
        ? "试试其他关键词或筛选条件。"
        : "由 Agent 制作任务后，在这里查看并试跑。"}
    </p>
    {#if search || filter !== "all"}<Button
        variant="outline"
        class="task-clear"
        onclick={() => {
          search = "";
          filter = "all";
        }}>清除筛选</Button
      >{/if}
  </div>
{:else}
  <div class="task-grid">
    {#each visibleTasks as task (task.workflow.id)}
      {@const workflow = task.workflow}
      {@const Icon = taskIcon(workflow)}
      {@const description =
        workflow.id === "swipe"
          ? "保持当前页面，按设置的时长和间隔上滑。"
          : workflow.description}
      <article class="task-card" aria-label={workflow.name}>
        <div class="task-card-heading">
          <span class="task-icon" aria-hidden="true"><Icon size={17} /></span>
          <h2>{workflow.name}</h2>
          <span class="task-availability" class:ready={!!task.active}
            >{task.active ? "已启用" : "待试跑"}</span
          >
        </div>
        <p class="task-description" title={description}>{description}</p>
        <div class="task-steps" aria-label="任务步骤">
          {#each workflow.nodes.slice(0, 4) as node}<span title={node.label}
              >{workflow.id === "swipe" && node.type === "swipe_loop"
                ? "按时长与间隔上滑"
                : node.label}</span
            >{/each}
          {#if workflow.nodes.length > 4}<span
              >另 {workflow.nodes.length - 4} 步</span
            >{/if}
        </div>
        <div class="task-meta">
          <span
            >设备 <b
              >{workflow.devices.length
                ? workflow.devices.map(deviceName).join("、")
                : "任选手机"}</b
            ></span
          >
          <span
            >触发 <b
              >{task.scheduled ? `手动／定时 ${task.scheduled} 条` : "手动"}</b
            ></span
          >
          <span title="当前保留的执行记录，包含试跑"
            >记录 <b>{task.history.length}</b> 次</span
          >
          <span
            >最近 <b
              >{task.latestRun
                ? time(task.latestRun.started_at)
                : "暂无记录"}</b
            ></span
          >
          <span>版本 <b>第 {workflow.version} 版</b></span>
          {#if task.latest.version > workflow.version}<button
              class="task-new-version"
              onclick={() => onOpen(task.latest)}
              >新版待试跑 · 第 {task.latest.version} 版</button
            >{/if}
        </div>
        {#if task.inProgress.length}
          <div class="task-live-runs">
            {#each task.inProgress as run (run.id)}
              <div>
                <button class="task-progress" onclick={() => onViewRuns(run)}
                  >{deviceName(run.device_id)} · {runState(run.state)}</button
                ><Button
                  variant="outline"
                  class="stop-button"
                  disabled={!!working}
                  onclick={() => onStop(run)}><Square size={12} />停止</Button
                >
              </div>
            {/each}
          </div>
        {/if}
        <div class="task-card-footer">
          <span
            class="task-result"
            class:running={task.inProgress.length > 0}
            class:success={!task.inProgress.length &&
              task.latestRun?.state === "succeeded"}
            class:failed={!task.inProgress.length &&
              task.latestRun?.state === "needs_help"}
          >
            {task.inProgress.length
              ? `${task.inProgress.length} 个进行中`
              : task.latestRun
                ? runState(task.latestRun.state)
                : "暂无记录"}
          </span>
          <div class="task-buttons">
            <Button variant="outline" onclick={() => onOpen(workflow)}
              ><Play size={12} />{task.active ? "运行" : "试跑"}</Button
            >
            <Button onclick={() => onOpen(task.latest)}>详情</Button>
            <Button
              variant="outline"
              disabled={!task.latestRun}
              onclick={() => {
                if (task.latestRun) onViewRuns(task.latestRun);
              }}><History size={12} />记录</Button
            >
          </div>
        </div>
      </article>
    {/each}
  </div>
{/if}
