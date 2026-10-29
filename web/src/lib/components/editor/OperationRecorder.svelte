<script lang="ts">
  import { onMount } from "svelte";
  import { Circle, Pause, Play, Check, X } from "@lucide/svelte";
  import LiveScreen from "$lib/components/LiveScreen.svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import { api, type Device } from "$lib/api";
  import {
    RecordingClock,
    recordingAction,
    recordingSteps,
    sameScreen,
    type RecordedAction,
    type RecordedInput,
    type RecordingScreen,
  } from "$lib/operation-recording";

  let {
    devices,
    initialDevice,
    capacity,
    oncomplete,
    onclose,
  }: {
    devices: Device[];
    initialDevice: string;
    capacity: number;
    oncomplete: (actions: RecordedAction[], device: string) => void;
    onclose: () => void;
  } = $props();
  let dialog: HTMLDialogElement;
  let device = $state("");
  let controlled = $state(false);
  let phase = $state<"ready" | "starting" | "recording" | "paused">("ready");
  let actions = $state<RecordedAction[]>([]);
  let screen: RecordingScreen | null = null;
  let pending = $state(false);
  let error = $state("");
  let discard = $state(false);
  let elapsed = $state(0);
  const recordingClock = new RecordingClock();
  let generation = 0;
  let locked = $state(false);
  const phone = $derived(devices.find((item) => item.id === device));
  const nodeCount = $derived(recordingSteps(actions).length);
  const canContinue = $derived(
    capacity - nodeCount >=
      (actions.length &&
      ["back", "home", "recents"].includes(actions.at(-1)!.kind)
        ? 2
        : 1),
  );
  const clock = $derived(
    `${Math.floor(elapsed / 60000)
      .toString()
      .padStart(2, "0")}:${Math.floor((elapsed / 1000) % 60)
      .toString()
      .padStart(2, "0")}`,
  );

  function pause(reason = "") {
    if (phase === "recording") {
      recordingClock.pause(performance.now());
      elapsed = recordingClock.at(performance.now());
    }
    if (phase === "recording" || phase === "starting") {
      generation++;
      phase = "paused";
      if (reason) error = reason;
    }
  }
  async function start() {
    if (!controlled || pending || !canContinue) return;
    error = "";
    discard = false;
    phase = "starting";
    locked = true;
    const attempt = ++generation;
    try {
      const result = await api<{ screen: RecordingScreen }>(
        "/editor/observe/" + encodeURIComponent(device),
      );
      if (generation !== attempt || phase !== "starting") return;
      if (!controlled || !phone?.online)
        throw new Error("手机操作尚未就绪，请重新投屏");
      if (result.screen.input_display !== 0)
        throw new Error("当前投屏无法确认这块屏幕的位置，请使用主屏录制");
      if (screen && !sameScreen(screen, result.screen))
        throw new Error(
          "手机屏幕形态已改变，请恢复原来的屏幕形态，或完成这段录制",
        );
      screen = result.screen;
      recordingClock.resume(performance.now());
      phase = "recording";
    } catch (e) {
      if (generation === attempt) {
        phase = "paused";
        error = e instanceof Error ? e.message : "无法开始录制";
      }
    }
  }
  async function receive(input: RecordedInput, confirmed: Promise<void>) {
    if (phase !== "recording" || !screen) {
      void confirmed.catch(() => {});
      return;
    }
    pending = true;
    const mapped = {
      ...input,
      started: recordingClock.at(input.started),
      ended: recordingClock.at(input.ended),
    };
    try {
      // 即使手势无法转换，也要接住控制通道的拒绝，避免未处理的异步错误。
      await confirmed;
      const action = recordingAction(mapped, device, screen);
      const next = [...actions, action];
      if (recordingSteps(next).length > capacity)
        throw new Error("画布最多 200 步；本次动作未记录，请完成已有记录");
      actions = next;
      if (
        capacity - recordingSteps(next).length <
        (["back", "home", "recents"].includes(action.kind) ? 2 : 1)
      )
        pause("已达到可录制步骤上限，请完成已有记录");
    } catch (e) {
      const reason =
        e instanceof Error ? e.message : "动作未能记录，请核对手机画面";
      pause(reason);
      error = reason;
    } finally {
      pending = false;
    }
  }
  function finish() {
    if (pending || !actions.length) return;
    pause();
    oncomplete(actions, device);
  }
  function close() {
    pause();
    if (actions.length || pending) discard = true;
    else onclose();
  }
  onMount(() => {
    device =
      devices.find((item) => item.id === initialDevice && item.online)?.id ||
      devices.find((item) => item.online)?.id ||
      "";
    dialog.showModal();
    const timer = setInterval(() => {
      if (phase === "recording") elapsed = recordingClock.at(performance.now());
    }, 250);
    const leaving = (event: BeforeUnloadEvent) => {
      if (actions.length || pending || phase === "recording")
        event.preventDefault();
    };
    window.addEventListener("beforeunload", leaving);
    return () => {
      generation++;
      clearInterval(timer);
      window.removeEventListener("beforeunload", leaving);
    };
  });
</script>

<dialog
  bind:this={dialog}
  class="operation-recorder"
  aria-labelledby="recorder-title"
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
>
  <header>
    <div>
      <h2 id="recorder-title">录制操作</h2>
      <p>操作会实时作用于手机；完成后将步骤放到画布右侧。</p>
    </div>
    <button class="close" aria-label="关闭录制面板" onclick={close}
      ><X size={20} /></button
    >
  </header>
  <div class="recorder-layout">
    <section class="phone-panel" aria-label="录制手机投屏">
      {#if phone}
        {#key device}<LiveScreen
            id={phone.id}
            name={phone.name}
            online={phone.online}
            fill
            showBrightness={false}
            bind:controlled
            inputEnabled={phase === "recording" && !pending}
            onrecordinput={receive}
            oninterrupt={pause}
          />{/key}
      {:else}<p class="empty">请先连接手机，再选择目标手机。</p>{/if}
    </section>
    <section class="record-panel" aria-label="录制记录">
      <label class="device-select"
        >目标手机<Dropdown
          label="录制目标手机"
          bind:value={device}
          disabled={locked}
          items={devices.map((item) => ({
            value: item.id,
            label: `${item.name}${item.online ? "" : "（离线）"}`,
            disabled: !item.online,
          }))}
        /></label
      >

      <div class="record-status" role="status">
        <Circle
          size={12}
          fill={phase === "recording" ? "currentColor" : "none"}
        />{phase === "recording"
          ? "录制中"
          : phase === "starting"
            ? "正在核对屏幕…"
            : phase === "paused"
              ? "已暂停"
              : "准备录制"}<time>{clock}</time>
      </div>
      <p class="count">{actions.length} 个动作 · 将生成 {nodeCount} 个节点</p>
      <p class="hint recording-help">
        点击、直线滑动、导航键及动作间等待。先开启投屏，再开始录制。
      </p>
      {#if !canContinue}<p class="hint">
          画布最多 200 步，已无足够空间继续录制。
        </p>{/if}
      {#if error}<p class="record-error" role="alert">{error}</p>{/if}
      {#if pending}<p class="hint" role="status">正在确认最后一个动作…</p>{/if}
      <div class="recorder-actions">
        {#if phase === "recording"}<button onclick={() => pause()}
            ><Pause size={16} />暂停</button
          >
        {:else}<button
            class="primary"
            disabled={!controlled ||
              pending ||
              phase === "starting" ||
              !canContinue}
            onclick={start}
            ><Play size={16} />{locked ? "继续录制" : "开始录制"}</button
          >{/if}
        <button
          class="primary"
          disabled={!actions.length || pending || phase === "starting"}
          onclick={finish}><Check size={16} />完成并加入画布</button
        >
        <button onclick={close}>放弃</button>
      </div>
      <ol class="action-list" aria-label="已记录动作">
        {#each actions as action, i}<li>
            <span>{i + 1}</span><strong
              >{(
                {
                  tap: "定点点击",
                  swipe: "滑动",
                  back: "返回键",
                  home: "主页键",
                  recents: "最近任务",
                } as Record<string, string>
              )[action.kind]}</strong
            >{#if action.kind === "swipe"}<small
                >{Number(action.config.duration_ms) / 1000} 秒</small
              >{/if}
          </li>{/each}
      </ol>
      {#if !actions.length}<p class="empty">录到的动作会出现在这里</p>{/if}
      {#if discard}<div class="discard-box" role="alert">
          <p>放弃这次记录？已发生的手机操作无法撤回。</p>
          <div>
            <button disabled={pending} onclick={onclose}>确认放弃</button
            ><button onclick={() => (discard = false)}>保留记录</button>
          </div>
        </div>{/if}
      <p class="hint recording-footer">
        生成后可整组撤销。刷新或离开前，请先完成录制；接入流程后才能保存运行。
      </p>
    </section>
  </div>
</dialog>

<style>
  .operation-recorder {
    inset: 0;
    margin: 0;
    width: 100dvw;
    height: 100dvh;
    max-width: none;
    max-height: none;
    padding: var(--workspace-gap);
    border: 0;
    border-radius: 0;
    background: var(--panel);
    color: var(--text);
    overflow: hidden;
  }
  .operation-recorder[open] {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: var(--workspace-gap);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 18px;
  }
  p {
    margin: 0;
    line-height: 1.6;
  }
  header p,
  .hint,
  .count,
  .empty {
    color: var(--dim);
    font-size: 13px;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 44px;
    padding: 8px 12px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--panel);
    color: var(--text);
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    border-color: var(--accent);
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .primary {
    background: var(--accent);
    color: var(--primary-ink);
    border-color: var(--accent);
  }
  .close {
    min-width: 44px;
  }
  .recorder-layout {
    display: grid;
    min-height: 0;
    grid-template-columns: minmax(0, 1fr) clamp(300px, 23vw, 380px);
    gap: var(--workspace-gap);
  }
  .phone-panel,
  .record-panel {
    min-width: 0;
    min-height: 0;
  }
  .phone-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .phone-panel :global(.device-row) {
    flex-wrap: wrap;
    gap: 12px;
  }
  .device-select {
    display: grid;
    gap: 8px;
    margin-bottom: 4px;
    font-size: 13px;
  }
  .record-panel {
    padding-left: var(--workspace-gap);
    border-left: 1px solid var(--line);
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .record-status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    color: var(--accent);
  }
  time {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }
  .action-list {
    list-style: none;
    padding: 0;
    margin: 0;
    flex: 1;
    min-height: 48px;
    overflow: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 0;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
  }
  li span {
    color: var(--dim);
    width: 24px;
  }
  li small {
    margin-left: auto;
    color: var(--dim);
  }
  .empty {
    padding: 20px 0;
  }
  .recorder-actions,
  .discard-box div {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .record-error,
  .discard-box {
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 9px;
    font-size: 13px;
  }
  .record-error {
    color: var(--red);
  }
  .discard-box p {
    margin-bottom: 8px;
  }
  @media (max-width: 760px) {
    .recorder-layout {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(0, 1fr) minmax(180px, 36%);
    }
    .operation-recorder {
      padding: 14px;
    }
    .record-panel {
      padding: 12px 0 0;
      border-left: 0;
      border-top: 1px solid var(--line);
    }
    .record-panel {
      gap: 8px;
    }
    .device-select {
      grid-template-columns: auto minmax(0, 1fr);
      align-items: center;
    }
    .recording-help,
    .recording-footer {
      display: none;
    }
    .record-panel > :global(*) {
      flex-shrink: 0;
    }
    .action-list {
      max-height: 140px;
    }
    header p {
      display: none;
    }
  }
</style>
