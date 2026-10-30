<script lang="ts">
  import { onMount } from "svelte";
  import {
    LoaderCircle,
    Maximize,
    Monitor,
    Square,
    ArrowLeft,
    Circle,
    PanelsTopLeft,
    ArrowUp,
    ArrowDown,
    ArrowRight,
    Minimize,
  } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { notify } from "$lib/notifications";
  import { startMirror } from "$lib/mirror";
  import {
    rememberMirror,
    forgetMirror,
    restoreMirror,
  } from "$lib/mirror-session";
  import { screenPoint, type ScreenPoint } from "$lib/touch";

  import type { RecordedInput } from "$lib/operation-recording";

  let {
    id,
    name,
    online,
    picture,
    dimensions = $bindable(""),
    controlled = $bindable(false),
    screenOff = $bindable(false),
    powerPending = $bindable(false),
    inputEnabled = true,
    fill = false,
    showBrightness = true,
    onrecordinput,
    oninterrupt,
  }: {
    id: string;
    name: string;
    online: boolean;
    picture?: string;
    dimensions?: string;
    controlled?: boolean;
    screenOff?: boolean;
    powerPending?: boolean;
    inputEnabled?: boolean;
    fill?: boolean;
    showBrightness?: boolean;
    onrecordinput?: (input: RecordedInput, confirmed: Promise<void>) => void;
    oninterrupt?: (reason: string) => void;
  } = $props();
  let canvas: HTMLCanvasElement;
  let stage: HTMLDivElement;
  let viewState = $state<
    "idle" | "connecting" | "live" | "error" | "taken_over"
  >("idle");
  let restorePending = $state(false);
  let supported = $state(false);
  let canFullscreen = $state(false);
  let isFullscreen = $state(false);
  let brightness = $state(100);
  let screenAspect = $state(0.5);
  const displayBrightness = $derived(showBrightness ? brightness : 100);
  const brightnessKey = "adb-studio-screen-brightness";
  let requesting = $state(false);
  let gesture = $state(false);
  let pointer = $state<number | undefined>();
  let recordedPoints: ScreenPoint[] = [];
  let gestureStarted = 0;
  let lastPoint: ScreenPoint | undefined;
  let pendingPoint: ScreenPoint | undefined;
  let moveFrame = 0;
  let gestureVersion = 0;
  let connection: ReturnType<typeof startMirror> | undefined;
  let pendingPower: boolean | undefined;

  function setBrightness(value: number) {
    brightness = Math.min(150, Math.max(5, value));
    try {
      localStorage.setItem(brightnessKey, String(brightness));
    } catch {
      // 浏览器禁用存储时，仍可调节当前画面。
    }
  }

  function record(input: RecordedInput) {
    if (onrecordinput && connection)
      onrecordinput(input, connection.checkpoint());
  }
  function cancelTouch() {
    if (lastPoint) oninterrupt?.("手势已中断，未完成动作没有记录");
    recordedPoints = [];
    gestureVersion++;
    gesture = false;
    cancelAnimationFrame(moveFrame);
    moveFrame = 0;
    pendingPoint = undefined;
    if (lastPoint) connection?.touch(3, lastPoint);
    lastPoint = undefined;
    const captured = pointer;
    pointer = undefined;
    if (captured !== undefined && canvas.hasPointerCapture(captured))
      canvas.releasePointerCapture(captured);
  }
  export function setDisplayPower(enabled: boolean) {
    if (powerPending || !online) return;
    if (!supported) {
      notify.error(
        "屏幕开关需要投屏通道，请使用本机地址或 HTTPS，并使用支持投屏的浏览器",
      );
      return;
    }
    if (viewState !== "live" && viewState !== "connecting") start();
    cancelTouch();
    powerPending = true;
    if (controlled) connection?.setDisplayPower(enabled);
    else {
      pendingPower = enabled;
      if (viewState === "live" && !requesting) requestControl();
    }
  }
  function endControl() {
    pendingPower = undefined;
    powerPending = false;
    requesting = false;
    cancelTouch();
    controlled = false;
    connection?.setControl(false);
  }
  function disconnect() {
    endControl();
    connection?.stop();
    connection = undefined;
    viewState = "idle";
    dimensions = "";
    powerPending = false;
  }
  function stop() {
    restorePending = false;
    forgetMirror(id);
    disconnect();
  }
  function start() {
    restorePending = false;
    disconnect();
    rememberMirror(id);
    viewState = "connecting";
    connection = startMirror(
      id,
      canvas,
      (width, height) => {
        const firstFrame = viewState === "connecting";
        viewState = "live";
        dimensions = `${width} × ${height}`;
        screenAspect = width / height;
        if (firstFrame) requestControl();
      },
      (message, reason) => {
        forgetMirror(id);
        viewState = reason === "taken_over" ? "taken_over" : "error";
        oninterrupt?.(message);
        if (reason !== "taken_over") notify.error(message);
        dimensions = "";
        pendingPower = undefined;
        powerPending = false;
        connection = undefined;
      },
      (enabled, message) => {
        controlled = enabled;
        requesting = false;
        if (enabled && pendingPower !== undefined) {
          const target = pendingPower;
          pendingPower = undefined;
          connection?.setDisplayPower(target);
        }
        if (!enabled) {
          oninterrupt?.(message || "操作权已断开，请重新开启投屏后继续");
          pendingPower = undefined;
          powerPending = false;
          cancelTouch();
        }
        if (message) notify.error(message);
      },
      (width, height) => {
        screenAspect = width / height;
        oninterrupt?.("投屏尺寸已变化，请核对屏幕后继续");
        if (lastPoint)
          lastPoint = {
            x: Math.min(lastPoint.x, width - 1),
            y: Math.min(lastPoint.y, height - 1),
            width,
            height,
          };
        if (controlled || requesting) {
          // 旋转后的新尺寸用于取消旧触点，避免继续按旧坐标操作。
          endControl();
          requestControl();
        }
      },
      (enabled, initial) => {
        screenOff = !enabled;
        if (!initial) powerPending = false;
      },
    );
  }
  function requestControl() {
    requesting = true;
    connection?.setControl(true);
  }
  function toggleFullscreen() {
    if (isFullscreen) {
      void document.exitFullscreen();
    } else {
      stage.requestFullscreen().catch(() => {
        notify.error("暂时无法全屏，请在浏览器中重试");
      });
    }
  }
  function point(event: PointerEvent, clamp = false) {
    const rect = canvas.getBoundingClientRect();
    return screenPoint(
      {
        left: rect.left + canvas.clientLeft,
        top: rect.top + canvas.clientTop,
        width: canvas.clientWidth,
        height: canvas.clientHeight,
      },
      canvas.width,
      canvas.height,
      event.clientX,
      event.clientY,
      clamp,
    );
  }
  function down(event: PointerEvent) {
    if (!event.isPrimary && onrecordinput && inputEnabled) {
      oninterrupt?.("暂不支持多指操作；已暂停录制");
      cancelTouch();
      return;
    }
    if (
      !inputEnabled ||
      !controlled ||
      gesture ||
      pointer !== undefined ||
      !event.isPrimary ||
      event.button !== 0
    )
      return;
    const target = point(event);
    if (!target) return;
    event.preventDefault();
    canvas.focus({ preventScroll: true });
    pointer = event.pointerId;
    lastPoint = target;
    canvas.setPointerCapture(event.pointerId);
    if (connection?.touch(0, target)) {
      gestureStarted = performance.now();
      recordedPoints = onrecordinput ? [target] : [];
    }
  }
  function move(event: PointerEvent) {
    if (pointer !== event.pointerId) return;
    event.preventDefault();
    pendingPoint = point(event, true);
    if (!moveFrame)
      moveFrame = requestAnimationFrame(() => {
        moveFrame = 0;
        if (pointer !== undefined && pendingPoint) {
          lastPoint = pendingPoint;
          if (connection?.touch(2, pendingPoint) && onrecordinput) {
            recordedPoints.push(pendingPoint);
            if (recordedPoints.length > 512) {
              oninterrupt?.("手势过长；本次手势未记录");
              cancelTouch();
            }
          }
          pendingPoint = undefined;
        }
      });
  }
  function up(event: PointerEvent) {
    if (pointer !== event.pointerId) return;
    event.preventDefault();
    cancelAnimationFrame(moveFrame);
    moveFrame = 0;
    pendingPoint = undefined;
    const target = point(event, true) || lastPoint;
    const sent = target && connection?.touch(1, target);
    const points = target ? [...recordedPoints, target] : [];
    recordedPoints = [];
    lastPoint = undefined;
    pointer = undefined;
    if (canvas.hasPointerCapture(event.pointerId))
      canvas.releasePointerCapture(event.pointerId);
    if (sent && inputEnabled)
      record({
        type: "gesture",
        points,
        started: gestureStarted,
        ended: performance.now(),
      });
  }
  function navigate(key: 3 | 4 | 187) {
    if (inputEnabled && controlled && !gesture && pointer === undefined) {
      const started = performance.now();
      if (connection?.navigate(key))
        record({ type: "key", key, started, ended: performance.now() });
    }
  }
  function swipe(direction: "up" | "down" | "left" | "right") {
    if (!inputEnabled || !controlled || gesture || pointer !== undefined)
      return;
    gesture = true;
    const version = ++gestureVersion;
    const vertical = direction === "up" || direction === "down";
    const reverse = direction === "down" || direction === "right";
    const start = reverse ? 0.25 : 0.75,
      end = reverse ? 0.75 : 0.25;
    const position = (progress: number): ScreenPoint => ({
      x: Math.floor(
        canvas.width * (vertical ? 0.5 : start + (end - start) * progress),
      ),
      y: Math.floor(
        canvas.height * (vertical ? start + (end - start) * progress : 0.5),
      ),
      width: canvas.width,
      height: canvas.height,
    });
    lastPoint = position(0);
    connection?.touch(0, lastPoint);
    const started = performance.now();
    const first = { ...lastPoint };
    function step(now: number) {
      if (version !== gestureVersion || !controlled || !inputEnabled) return;
      const progress = Math.min(1, (now - started) / 280);
      lastPoint = position(progress);
      const sent = connection?.touch(progress === 1 ? 1 : 2, lastPoint);
      if (progress === 1) {
        lastPoint = undefined;
        gesture = false;
        if (sent)
          record({
            type: "gesture",
            points: [first, position(1)],
            started,
            ended: now,
          });
      } else requestAnimationFrame(step);
    }
    requestAnimationFrame(step);
  }
  function keyboard(event: KeyboardEvent) {
    if (!controlled || !inputEnabled) return;
    const directions = {
      ArrowUp: "up",
      ArrowDown: "down",
      ArrowLeft: "left",
      ArrowRight: "right",
    } as const;
    if (event.key in directions) {
      event.preventDefault();
      if (!event.repeat)
        swipe(directions[event.key as keyof typeof directions]);
    }
  }
  onMount(() => {
    try {
      const saved = showBrightness
        ? Number(localStorage.getItem(brightnessKey))
        : 100;
      if (Number.isFinite(saved) && saved >= 5 && saved <= 150)
        brightness = saved;
    } catch {
      // 无法读取浏览器存储时使用默认亮度。
    }
    supported = window.isSecureContext && "VideoDecoder" in window;
    canFullscreen = document.fullscreenEnabled;
    const navigation = performance.getEntriesByType("navigation")[0] as
      PerformanceNavigationTiming | undefined;
    const entry = new URL(navigation?.name || location.href);
    const devicePage =
      entry.pathname === "/" &&
      (entry.searchParams.get("tab") || "devices") === "devices";
    restorePending = restoreMirror(
      id,
      devicePage ? navigation?.type || "navigate" : "navigate",
      undefined,
      document,
    );
    if (!restorePending) forgetMirror(id);
    let leavingDocument = false;
    const pageHidden = () => {
      leavingDocument = true;
      disconnect();
    };
    const pageShown = (event: PageTransitionEvent) => {
      leavingDocument = false;
      if (event.persisted) {
        stop();
      }
    };
    const visibility = () => {
      if (document.hidden) {
        oninterrupt?.("页面已隐藏，录制已暂停");
        cancelTouch();
      }
    };
    const fullscreenChanged = () => {
      isFullscreen = document.fullscreenElement === stage;
    };
    document.addEventListener("visibilitychange", visibility);
    document.addEventListener("fullscreenchange", fullscreenChanged);
    const blurred = () => {
      oninterrupt?.("窗口已失去焦点，录制已暂停");
      cancelTouch();
    };
    window.addEventListener("blur", blurred);
    window.addEventListener("pagehide", pageHidden);
    window.addEventListener("pageshow", pageShown);
    return () => {
      document.removeEventListener("visibilitychange", visibility);
      document.removeEventListener("fullscreenchange", fullscreenChanged);
      window.removeEventListener("blur", blurred);
      window.removeEventListener("pagehide", pageHidden);
      window.removeEventListener("pageshow", pageShown);
      if (leavingDocument) disconnect();
      else stop();
    };
  });
  $effect(() => {
    if (!inputEnabled) cancelTouch();
  });
  $effect(() => {
    if (!online) {
      if (viewState === "connecting" || viewState === "live") stop();
    } else if (restorePending && supported) {
      start();
    }
  });
</script>

{#snippet swipeControls()}
  <div class="live-swipes" role="group" aria-label="手机滑动">
    <Button
      variant="outline"
      size="icon"
      aria-label="上滑"
      title="上滑"
      disabled={!inputEnabled ||
        !controlled ||
        gesture ||
        pointer !== undefined}
      onclick={() => swipe("up")}><ArrowUp size={20} /></Button
    >
    <Button
      variant="outline"
      size="icon"
      aria-label="下滑"
      title="下滑"
      disabled={!inputEnabled ||
        !controlled ||
        gesture ||
        pointer !== undefined}
      onclick={() => swipe("down")}><ArrowDown size={20} /></Button
    >
    <Button
      variant="outline"
      size="icon"
      aria-label="左滑"
      title="左滑"
      disabled={!inputEnabled ||
        !controlled ||
        gesture ||
        pointer !== undefined}
      onclick={() => swipe("left")}><ArrowLeft size={20} /></Button
    >
    <Button
      variant="outline"
      size="icon"
      aria-label="右滑"
      title="右滑"
      disabled={!inputEnabled ||
        !controlled ||
        gesture ||
        pointer !== undefined}
      onclick={() => swipe("right")}><ArrowRight size={20} /></Button
    >
  </div>
{/snippet}

<div class="device-row">
  <div class="live-heading">
    <h2>手机屏幕</h2>
    {#if viewState === "live" || viewState === "connecting"}<Button
        variant="outline"
        onclick={stop}
        ><Square size={15} />{viewState === "connecting"
          ? "取消连接"
          : "停止投屏"}</Button
      >
    {:else}<Button disabled={!online || !supported} onclick={start}
        ><Monitor size={15} />{viewState === "error"
          ? "重新投屏"
          : viewState === "taken_over"
            ? "在此投屏"
            : "开启投屏"}</Button
      >{/if}
  </div>
  <div class="live-heading-actions">
    {#if viewState === "live" && !isFullscreen}{@render swipeControls()}{/if}
    <span
      class="device-status"
      class:offline={viewState !== "live"}
      role="status"
      ><span class="status-dot" aria-hidden="true"></span>{viewState === "live"
        ? controlled
          ? onrecordinput && !inputEnabled
            ? "仅观看"
            : "操作中"
          : "实时画面"
        : viewState === "connecting"
          ? "连接中"
          : "未投屏"}</span
    >
  </div>
</div>
<div
  class="live-stage"
  class:fill
  bind:this={stage}
  style:--live-aspect={screenAspect}
>
  <div class="device-screen-body">
    <div class="live-device" class:visible={viewState === "live"}>
      {#if viewState === "live"}
        <nav class="live-navigation" aria-label="手机导航键">
          <Button
            variant="ghost"
            size="icon"
            aria-label="返回"
            title="返回"
            disabled={!inputEnabled ||
              !controlled ||
              gesture ||
              pointer !== undefined}
            onclick={() => navigate(4)}><ArrowLeft size={20} /></Button
          >
          <Button
            variant="ghost"
            size="icon"
            aria-label="主页"
            title="主页"
            disabled={!inputEnabled ||
              !controlled ||
              gesture ||
              pointer !== undefined}
            onclick={() => navigate(3)}><Circle size={20} /></Button
          >
          <Button
            variant="ghost"
            size="icon"
            aria-label="最近任务"
            title="最近任务"
            disabled={!inputEnabled ||
              !controlled ||
              gesture ||
              pointer !== undefined}
            onclick={() => navigate(187)}><PanelsTopLeft size={20} /></Button
          >
          {#if canFullscreen && !fill}<Button
              variant="ghost"
              size="icon"
              aria-label={isFullscreen ? "退出全屏" : "全屏观看"}
              title={isFullscreen ? "退出全屏" : "全屏观看"}
              onclick={toggleFullscreen}
              >{#if isFullscreen}<Minimize size={20} />{:else}<Maximize
                  size={20}
                />{/if}</Button
            >{/if}
        </nav>
      {/if}
      <canvas
        bind:this={canvas}
        class="device-screenshot live-canvas"
        class:visible={viewState === "live"}
        class:controllable={controlled && inputEnabled}
        style:filter={`brightness(${displayBrightness}%)`}
        aria-label={`${name}实时屏幕${controlled && inputEnabled ? "，可点按拖动，方向键滑动" : "，仅观看"}`}
        tabindex={controlled && inputEnabled ? 0 : -1}
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={cancelTouch}
        onlostpointercapture={() => {
          if (pointer !== undefined) cancelTouch();
        }}
        onkeydown={keyboard}
      ></canvas>
    </div>
    {#if viewState === "connecting"}<div class="device-screen-placeholder">
        <LoaderCircle size={32} class="spin" /><strong>正在连接手机…</strong
        ><span>首次连接需要几秒钟</span>
      </div>
    {:else if viewState !== "live"}
      {#if picture && viewState !== "error"}<img
          class="device-screenshot"
          style:filter={`brightness(${displayBrightness}%)`}
          src={picture}
          alt={`${name}最近一次截图`}
        />
      {:else}<div class="device-screen-placeholder">
          <Monitor size={32} strokeWidth={1.4} /><strong
            >{viewState === "error"
              ? "投屏已中断"
              : viewState === "taken_over"
                ? "投屏已切换到其他页面"
                : "查看实时画面"}</strong
          ><span
            >{viewState === "taken_over"
              ? "点击“在此投屏”可切回当前页面"
              : online
                ? "解锁手机后，点击标题旁的开启投屏"
                : "手机连接后可开启投屏"}</span
          >
        </div>{/if}
    {/if}
  </div>
  {#if showBrightness}<div class="live-brightness">
      <label>
        <span>网页亮度</span>
        <input
          type="range"
          min="5"
          max="150"
          step="5"
          value={brightness}
          aria-valuetext={`${brightness}%`}
          oninput={(event) => setBrightness(event.currentTarget.valueAsNumber)}
        />
      </label>
      <span class="live-brightness-value">{brightness}%</span>
      <Button
        variant="ghost"
        size="sm"
        disabled={brightness === 100}
        onclick={() => setBrightness(100)}>恢复</Button
      >
      <span class="hint">仅影响网页画面</span>
    </div>
  {/if}
  {#if viewState === "live" && (!controlled || isFullscreen)}
    <div class="live-control-area">
      {#if !controlled && !requesting}<Button
          variant="outline"
          onclick={requestControl}>重试操作</Button
        >{:else if requesting}<span class="hint live-control-pending"
          ><LoaderCircle class="spin" size={16} />正在开启操作…</span
        >{/if}
      {#if isFullscreen}{@render swipeControls()}{/if}
    </div>
  {/if}
</div>
{#if !supported}<p class="hint">
    实时投屏需支持视频解码的浏览器。本机请使用 localhost 或
    127.0.0.1；其他设备需 HTTPS。
  </p>
{/if}
{#if !fill}<p class="hint live-footnote">
    {onrecordinput
      ? "操作会真实作用于手机；暂停录制时仅观看画面。"
      : "开启投屏会切换到当前页面；刷新自动恢复，离开设备页后停止投屏。"}
  </p>
{/if}

<style>
  .live-stage.fill {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .fill .device-screen-body {
    container-type: size;
    flex: 1;
    width: 100%;
    min-height: 0;
    margin: 0;
    overflow: hidden;
  }
  .fill .live-device {
    max-width: 100%;
    padding: 0;
    border-width: 1px;
    border-radius: 12px;
    box-shadow: none;
  }
  .fill .live-device .live-canvas {
    width: min(calc(100cqw - 2px), calc((100cqh - 48px) * var(--live-aspect)));
    height: auto;
    max-height: none;
    max-width: none;
    border-radius: 0 0 11px 11px;
  }
  .fill .live-navigation {
    height: 44px;
    grid-template-columns: repeat(3, minmax(44px, 1fr));
    align-items: center;
  }
  .fill .live-navigation :global(button[data-slot="button"]) {
    height: 40px;
  }
  .fill .device-screen-placeholder {
    width: min(100cqw, 360px);
    min-height: 0;
    height: min(100cqh, 540px);
    border-width: 1px;
    border-radius: 12px;
    box-shadow: none;
  }
  .fill .live-control-area {
    flex-shrink: 0;
    margin: 8px 0 0;
  }
</style>
