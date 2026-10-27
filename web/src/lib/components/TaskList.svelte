<script lang="ts">
  import {
    Hand,
    History,
    LoaderCircle,
    MousePointer2,
    Play,
    RefreshCw,
    Square,
    MoreHorizontal,
    Pencil,
    Archive,
    ArchiveRestore,
    Clock3,
    FileText,
    Layers,
    Plus,
    Search,
    Smartphone,
    Workflow as FlowIcon,
  } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import {
    busy,
    runLabel,
    time,
    type Device,
    type Run,
    type Schedule,
    type Workflow,
  } from "$lib/api";
  import { summarizeTasks } from "$lib/tasks";
  import type { Draft } from "$lib/workflow-editor";

  let {
    workflows,
    drafts,
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
    onArchive,
    onRestore,
  }: {
    workflows: Workflow[];
    drafts: Draft[];
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
    onArchive: (workflow: Workflow) => Promise<boolean>;
    onRestore: (workflow: Workflow) => Promise<boolean>;
  } = $props();

  let search = $state("");
  let filter = $state("all");
  let archiveTarget = $state<Workflow | null>(null);
  let archiveDialog = $state<HTMLDialogElement>();
  const tasks = $derived(summarizeTasks(workflows, runs, schedules));
  const currentTasks = $derived(
    tasks.filter((task) => !task.workflow.archived),
  );
  const draftIds = $derived(new Set(drafts.map((draft) => draft.id)));
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
      if (!text.includes(search.trim().toLowerCase())) return false;
      if (filter === "archived") return !!workflow.archived;
      if (workflow.archived) return false;
      return (
        filter === "all" || (filter === "running" && task.inProgress.length > 0)
      );
    }),
  );

  function deviceName(id: string) {
    return devices.find((device) => device.id === id)?.name || id;
  }

  function currentRun(inProgress: Run[]) {
    return inProgress.find((run) => run.state !== "paused") ?? inProgress[0];
  }

  function draftId(workflow: Workflow) {
    const id = workflow.id.slice("manual-".length);
    return workflow.id.startsWith("manual-") &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)
      ? id
      : "";
  }

  function editHref(workflow: Workflow) {
    const id = draftId(workflow);
    return id && draftIds.has(id)
      ? `/editor?draft=${encodeURIComponent(id)}`
      : `/workflow-edit?id=${encodeURIComponent(workflow.id)}`;
  }

  async function confirmArchive() {
    if (!archiveTarget) return;
    if (await onArchive(archiveTarget)) {
      archiveDialog?.close();
      archiveTarget = null;
    }
  }

  function closeMoreMenu(event: MouseEvent) {
    const menu = (event.currentTarget as HTMLElement).closest("details");
    if (menu) menu.open = false;
  }

  function closeMoreMenuOnEscape(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    const menu = (event.currentTarget as HTMLElement).closest("details");
    if (!menu) return;
    menu.open = false;
    menu.querySelector("summary")?.focus();
  }

  function closeMoreMenusOutside(event: MouseEvent) {
    if (!(event.target instanceof Node)) return;
    for (const menu of document.querySelectorAll<HTMLDetailsElement>(
      ".catalog-more[open]",
    )) {
      if (!menu.contains(event.target)) menu.open = false;
    }
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

<svelte:window onclick={closeMoreMenusOutside} />

<section class="workflow-catalog" aria-label="自动化任务">
  <header class="catalog-heading">
    <h1>自动化任务</h1>
    <div class="catalog-toolbar" aria-label="搜索与筛选任务">
      <div class="catalog-search">
        <Search size={17} aria-hidden="true" />
        <label class="visually-hidden" for="task-search"
          >搜索任务名称或手机</label
        >
        <input
          id="task-search"
          type="search"
          placeholder="搜索任务名称或手机"
          bind:value={search}
        />
      </div>
      <div class="catalog-filter">
        <Dropdown
          label="筛选任务状态"
          bind:value={filter}
          items={[
            { value: "all", label: "全部任务" },
            { value: "running", label: "未结束" },
            { value: "archived", label: "已归档" },
          ]}
        />
      </div>
      <div class="catalog-tools">
        <Button
          variant="outline"
          disabled={!connected || !!working}
          onclick={onRefresh}
        >
          <RefreshCw
            size={15}
            aria-hidden="true"
            class={working === "refresh-tasks" ? "spin" : ""}
          />刷新
        </Button>
        <Button href="/editor" class="catalog-create"
          ><Plus size={16} aria-hidden="true" />编排工作流</Button
        >
        {#if currentTasks.some((task) => task.inProgress.length)}
          <Button
            variant="outline"
            class="stop-button"
            disabled={!!working || !connected}
            onclick={onStopAll}
          >
            <Square size={14} aria-hidden="true" />全部停止
          </Button>
        {/if}
      </div>
    </div>
  </header>

  {#if loading}
    <div class="empty"><LoaderCircle class="spin" />正在读取任务…</div>
  {:else if !visibleTasks.length}
    <div class="empty">
      <FlowIcon size={28} />
      <h2>{tasks.length ? "没有符合条件的任务" : "尚未添加任务"}</h2>
      <p>
        {tasks.length
          ? filter === "all" && tasks.some((task) => task.workflow.archived)
            ? "可在筛选中查看已归档任务。"
            : "试试其他关键词或筛选条件。"
          : "点击“编排工作流”添加任务。"}
      </p>
      {#if search || filter !== "all"}
        <Button
          variant="outline"
          onclick={() => {
            search = "";
            filter = "all";
          }}>清除筛选</Button
        >
      {/if}
    </div>
  {:else}
    <div class="catalog-grid">
      {#each visibleTasks as task (task.workflow.id)}
        {@const workflow = task.workflow}
        {@const Icon = taskIcon(workflow)}
        {@const run = currentRun(task.inProgress)}
        {@const description =
          workflow.id === "swipe"
            ? "保持当前页面，按设置的时长和间隔上滑。"
            : workflow.description ||
              `共 ${workflow.nodes.length} 个步骤，点击任务名称查看详情。`}
        {@const status = workflow.archived
          ? "已归档"
          : run
            ? runLabel(run)
            : "已保存"}
        <article class="catalog-card" aria-label={workflow.name}>
          <div class="catalog-card-heading">
            <span class="catalog-icon" aria-hidden="true"
              ><Icon size={20} /></span
            >
            <h2>
              <button
                onclick={() => onOpen(task.workflow)}
                aria-label={`查看任务详情：${workflow.name}`}
                >{workflow.name}</button
              >
            </h2>
            <span
              class="catalog-status"
              class:ready={!run && !workflow.archived}
              class:live={!!run && run.state !== "paused"}
              class:paused={run?.state === "paused"}
              class:archived={!!workflow.archived}>{status}</span
            >
          </div>

          <p class="catalog-description" title={description}>{description}</p>

          <dl class="catalog-meta">
            <div>
              <dt><Smartphone size={15} aria-hidden="true" />设备</dt>
              <dd
                title={workflow.devices.length
                  ? workflow.devices.map(deviceName).join("、")
                  : "任选手机"}
              >
                {workflow.devices.length
                  ? workflow.devices.map(deviceName).join("、")
                  : "任选手机"}
              </dd>
            </div>
            <div>
              <dt><History size={15} aria-hidden="true" />最近执行</dt>
              <dd>
                {task.latestRun ? time(task.latestRun.started_at) : "暂无记录"}
              </dd>
            </div>
            <div>
              <dt><Clock3 size={15} aria-hidden="true" />触发</dt>
              <dd>
                {task.scheduled ? `手动／定时 ${task.scheduled} 条` : "手动"}
              </dd>
            </div>
            <div>
              <dt><Layers size={15} aria-hidden="true" />版本</dt>
              <dd>
                第 {workflow.version} 版
              </dd>
            </div>
          </dl>

          <footer class="catalog-footer">
            <span
              class="catalog-result"
              class:live={!!run && run.state !== "paused"}
              class:paused={run?.state === "paused"}
              class:success={!run && task.latestRun?.state === "succeeded"}
              class:attention={!run && task.latestRun?.state === "needs_help"}
              title={run
                ? task.inProgress
                    .map(
                      (item) =>
                        `${deviceName(item.device_id)} · ${runLabel(item)}`,
                    )
                    .join("；")
                : undefined}
            >
              <span class="catalog-result-dot" aria-hidden="true"></span>
              <span
                >{run
                  ? task.inProgress.length > 1
                    ? `${task.inProgress.length} 个未结束`
                    : `${deviceName(run.device_id)} · ${runLabel(run)}`
                  : task.latestRun
                    ? runLabel(task.latestRun)
                    : "尚未执行"}</span
              >
            </span>
            <div class="catalog-actions">
              {#if workflow.archived}
                <Button
                  class="catalog-primary"
                  disabled={!!working || !connected}
                  onclick={() => onRestore(workflow)}
                  ><ArchiveRestore
                    size={15}
                    aria-hidden="true"
                  />恢复任务</Button
                >
              {:else}
                <Button
                  variant="outline"
                  href={editHref(workflow)}
                  aria-label={`编辑任务：${workflow.name}`}
                  ><Pencil size={15} aria-hidden="true" />编辑</Button
                >
                <Button
                  class="catalog-primary"
                  onclick={() => (run ? onViewRuns(run) : onOpen(workflow))}
                >
                  {#if run}<History
                      size={15}
                      aria-hidden="true"
                    />查看进度{:else}<Play
                      size={15}
                      aria-hidden="true"
                    />运行{/if}
                </Button>
              {/if}
              <details class="catalog-more">
                <summary
                  aria-label={`${workflow.name}的更多操作`}
                  title="更多操作"
                  onkeydown={closeMoreMenuOnEscape}
                  ><MoreHorizontal size={19} aria-hidden="true" /></summary
                >
                <div class="catalog-more-menu">
                  <button
                    onkeydown={closeMoreMenuOnEscape}
                    onclick={(event) => {
                      closeMoreMenu(event);
                      onOpen(task.workflow);
                    }}><FileText size={15} aria-hidden="true" />任务详情</button
                  >
                  <button
                    disabled={!task.latestRun}
                    onkeydown={closeMoreMenuOnEscape}
                    onclick={(event) => {
                      closeMoreMenu(event);
                      if (task.latestRun) onViewRuns(task.latestRun);
                    }}
                    ><History size={15} aria-hidden="true" />执行记录（{task
                      .history.length}）</button
                  >
                  {#if run}
                    <button
                      onkeydown={closeMoreMenuOnEscape}
                      onclick={(event) => {
                        closeMoreMenu(event);
                        onOpen(workflow);
                      }}><Play size={15} aria-hidden="true" />新建运行</button
                    >
                    {#each task.inProgress as item (item.id)}
                      <button
                        class="stop-button"
                        disabled={!!working || !connected}
                        onkeydown={closeMoreMenuOnEscape}
                        onclick={(event) => {
                          closeMoreMenu(event);
                          onStop(item);
                        }}
                        ><Square size={15} aria-hidden="true" />停止 · {deviceName(
                          item.device_id,
                        )}</button
                      >
                    {/each}
                  {/if}
                  {#if !workflow.archived}
                    <button
                      aria-label={`归档任务：${workflow.name}`}
                      disabled={!!working || !connected}
                      onkeydown={closeMoreMenuOnEscape}
                      onclick={(event) => {
                        closeMoreMenu(event);
                        archiveTarget = workflow;
                        archiveDialog?.showModal();
                      }}
                      ><Archive size={15} aria-hidden="true" />归档任务</button
                    >
                  {/if}
                </div>
              </details>
            </div>
          </footer>
        </article>
      {/each}
    </div>
  {/if}
</section>

<dialog
  bind:this={archiveDialog}
  class="task-archive-dialog"
  onclose={() => (archiveTarget = null)}
>
  {#if archiveTarget}
    <h2>归档“{archiveTarget.name}”？</h2>
    <p>
      任务将移到“已归档”；相关定时会停用。版本和执行记录仍保留，之后可以恢复。
    </p>
    {#if runs.some((run) => run.workflow.id === archiveTarget?.id && busy(run))}
      <p>请先停止正在运行或暂停的任务。</p>
    {/if}
    <div class="dialog-actions">
      <Button variant="outline" onclick={() => archiveDialog?.close()}
        >取消</Button
      >
      <Button
        disabled={!!working ||
          runs.some(
            (run) => run.workflow.id === archiveTarget?.id && busy(run),
          )}
        onclick={confirmArchive}>确认归档</Button
      >
    </div>
  {/if}
</dialog>

<style>
  .workflow-catalog {
    display: grid;
    gap: var(--workspace-gap);
    min-width: 0;
  }
  .catalog-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    flex-wrap: wrap;
  }
  .catalog-heading h1 {
    font-size: 26px;
    line-height: 1.3;
  }
  .catalog-toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    margin-left: auto;
  }
  .catalog-search {
    position: relative;
    width: 280px;
    min-width: 180px;
  }
  .catalog-search > :global(svg) {
    position: absolute;
    top: 50%;
    left: 13px;
    transform: translateY(-50%);
    pointer-events: none;
    color: var(--dim);
  }
  .catalog-search input {
    width: 100%;
    height: 40px;
    min-height: 40px;
    margin: 0;
    padding: 8px 12px 8px 39px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--panel);
    color: var(--text);
    font-size: 13px;
  }
  .catalog-search input::placeholder {
    color: var(--dim);
  }
  .catalog-filter {
    width: 150px;
    flex: none;
  }
  .catalog-filter :global(.dropdown-trigger) {
    height: 40px;
    min-height: 40px;
  }
  .catalog-tools {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    flex: none;
  }
  .catalog-heading :global([data-slot="button"]),
  .catalog-tools :global([data-slot="button"]) {
    height: 40px;
    min-height: 40px;
    padding: 0 14px;
    font-size: 13px;
    line-height: 1;
    transition-property: background-color, color, border-color, box-shadow;
  }
  .catalog-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--workspace-gap);
  }
  .catalog-card {
    container-type: inline-size;
    container-name: task-card;
    display: flex;
    flex-direction: column;
    gap: 18px;
    min-width: 0;
    padding: 22px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: var(--panel);
  }
  .catalog-card-heading {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 40px;
  }
  .catalog-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--accentsoft);
    color: var(--accent2);
  }
  .catalog-card h2 {
    flex: 1;
    min-width: 0;
    font-size: 16px;
    font-weight: 700;
    line-height: 1.4;
  }
  .catalog-card h2 button {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    max-width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    overflow-wrap: anywhere;
  }
  .catalog-card h2 button:hover {
    color: var(--accent2);
  }
  .catalog-status {
    flex: none;
    padding: 5px 9px;
    border-radius: 7px;
    background: color-mix(in srgb, var(--amber) 12%, transparent);
    color: var(--amber);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }
  .catalog-status.ready,
  .catalog-status.live {
    background: var(--accentsoft);
    color: var(--accent2);
  }
  .catalog-status.archived {
    background: var(--panel2);
    color: var(--dim);
  }
  .catalog-description {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    min-height: 42px;
    color: var(--dim);
    font-size: 13px;
    line-height: 21px;
  }
  .catalog-meta {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px 20px;
    margin: 0;
  }
  .catalog-meta > div {
    display: grid;
    grid-template-columns: 76px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    min-width: 0;
    margin: 0;
  }
  .catalog-meta dt {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--dim);
    font-size: 12px;
    font-weight: 400;
    white-space: nowrap;
  }
  .catalog-meta dt :global(svg) {
    flex: none;
  }
  .catalog-meta dd {
    display: block;
    overflow: hidden;
    min-width: 0;
    color: var(--text);
    font-size: 12px;
    font-weight: 550;
    line-height: 20px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .catalog-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: auto;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
  .catalog-result {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    color: var(--dim);
    font-size: 12px;
    line-height: 20px;
  }
  .catalog-result > span:last-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .catalog-result-dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
  }
  .catalog-result.live,
  .catalog-result.success {
    color: var(--accent2);
  }
  .catalog-result.paused,
  .catalog-result.attention {
    color: var(--amber);
  }
  .catalog-actions {
    display: flex;
    align-items: center;
    flex: none;
    gap: 8px;
  }
  .catalog-actions :global([data-slot="button"]) {
    height: 40px;
    min-height: 40px;
    padding: 0 12px;
    border-radius: 9px;
    font-size: 12px;
    font-weight: 600;
    line-height: 1;
    transition-property: background-color, color, border-color, box-shadow;
  }
  .catalog-actions :global([data-slot="button"] svg) {
    width: 15px;
    height: 15px;
  }
  .catalog-actions :global(.catalog-primary) {
    min-width: 104px;
  }
  .catalog-more {
    position: relative;
    display: flex;
    align-items: center;
    margin: 0;
  }
  .catalog-more summary {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--panel);
    color: var(--text);
    list-style: none;
  }
  .catalog-more summary::-webkit-details-marker {
    display: none;
  }
  .catalog-more summary:hover,
  .catalog-more[open] summary {
    border-color: var(--accent);
    color: var(--accent2);
  }
  .catalog-more-menu {
    position: absolute;
    z-index: 10;
    right: 0;
    bottom: calc(100% + 8px);
    display: grid;
    width: 220px;
    padding: 5px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--panel);
    box-shadow: var(--shadow);
  }
  .catalog-more-menu button {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    min-height: 40px;
    padding: 9px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text);
    font-size: 12px;
    text-align: left;
    overflow-wrap: anywhere;
  }
  .catalog-more-menu button.stop-button {
    color: var(--red);
  }
  .catalog-more-menu button:hover:not(:disabled) {
    background: var(--panel2);
  }
  .catalog-more-menu button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .task-archive-dialog {
    width: min(440px, calc(100vw - 32px));
    padding: 24px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  .task-archive-dialog::backdrop {
    background: rgb(0 0 0 / 55%);
  }
  .task-archive-dialog h2 {
    margin-bottom: 12px;
    font-size: 18px;
  }
  .task-archive-dialog p {
    margin-bottom: 14px;
    color: var(--dim);
    font-size: 13px;
    line-height: 1.6;
  }
  @container task-card (max-width: 430px) {
    .catalog-meta {
      column-gap: 12px;
    }
    .catalog-meta > div {
      grid-template-columns: 76px minmax(0, 1fr);
      gap: 8px;
    }
    .catalog-footer {
      flex-direction: column;
      align-items: stretch;
    }
    .catalog-actions {
      justify-content: flex-end;
    }
  }
  @container task-card (max-width: 320px) {
    .catalog-meta {
      grid-template-columns: 1fr;
      gap: 12px;
    }
    .catalog-meta > div:nth-child(2) {
      grid-row: 3;
    }
  }
  @media (max-width: 1100px) {
    .catalog-grid {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 900px) {
    .catalog-toolbar {
      width: 100%;
      justify-content: flex-end;
    }
    .catalog-search {
      flex: 1;
      width: auto;
    }
  }
  @media (max-width: 660px) {
    .catalog-heading {
      align-items: flex-start;
      flex-wrap: wrap;
      gap: 12px;
    }
    .catalog-heading h1 {
      font-size: 24px;
    }
    .catalog-toolbar {
      flex-wrap: wrap;
      gap: 10px;
    }
    .catalog-filter {
      width: 125px;
    }
    .catalog-tools {
      flex-basis: 100%;
      flex-wrap: wrap;
      max-width: 100%;
    }
    .catalog-card {
      padding: 18px;
    }
    .catalog-card h2 {
      font-size: 15px;
    }
    .catalog-card-heading {
      gap: 9px;
    }
    .catalog-icon {
      width: 34px;
      height: 34px;
    }
    .catalog-heading :global([data-slot="button"]),
    .catalog-tools :global([data-slot="button"]),
    .catalog-actions :global([data-slot="button"]),
    .catalog-filter :global(.dropdown-trigger),
    .catalog-search input,
    .catalog-more summary {
      height: 44px;
      min-height: 44px;
    }
    .catalog-more summary {
      width: 44px;
    }
    .catalog-more-menu button {
      min-height: 44px;
    }
  }
</style>
