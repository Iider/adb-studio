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
  import { startMirror } from "$lib/mirror";
  import { screenPoint, type ScreenPoint } from "$lib/touch";

  let {
    id,
    name,
    online,
    picture,
    dimensions = $bindable(""),
    controlled = $bindable(false),
    screenOff = $bindable(false),
    powerPending = $bindable(false),
  }: {
    id: string;
    name: string;
    online: boolean;
    picture?: string;
    dimensions?: string;
    controlled?: boolean;
    screenOff?: boolean;
    powerPending?: boolean;
  } = $props();
  let canvas: HTMLCanvasElement;
  let stage: HTMLDivElement;
  let viewState = $state<"idle" | "connecting" | "live" | "error">("idle");
  let error = $state("");
  let supported = $state(false);
  let canFullscreen = $state(false);
  let isFullscreen = $state(false);
  let requesting = $state(false);
  let gesture = $state(false);
  let pointer = $state<number | undefined>();
  let lastPoint: ScreenPoint | undefined;
  let pendingPoint: ScreenPoint | undefined;
  let moveFrame = 0;
  let gestureVersion = 0;
  let connection: ReturnType<typeof startMirror> | undefined;
  let pendingPower: boolean | undefined;

  function cancelTouch() {
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
      error = "屏幕开关需要投屏通道，请使用本机地址或 HTTPS，并使用支持投屏的浏览器";
      return;
    }
    if (viewState === "idle" || viewState === "error") start();
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
  function stop() {
    endControl();
    connection?.stop();
    connection = undefined;
    viewState = "idle";
    dimensions = "";
    powerPending = false;
  }
  function start() {
    stop();
    error = "";
    viewState = "connecting";
    connection = startMirror(
      id,
      canvas,
      (width, height) => {
        const firstFrame = viewState === "connecting";
        viewState = "live";
        dimensions = `${width} × ${height}`;
        if (firstFrame) requestControl();
      },
      (message) => {
        viewState = "error";
        error = message;
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
          pendingPower = undefined;
          powerPending = false;
          cancelTouch();
        }
        if (message) error = message;
      },
      (width, height) => {
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
    error = "";
    requesting = true;
    connection?.setControl(true);
  }
  function toggleFullscreen() {
    if (isFullscreen) {
      void document.exitFullscreen();
    } else {
      stage.requestFullscreen().catch(() => {
        error = "暂时无法全屏，请在浏览器中重试";
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
    if (
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
    connection?.touch(0, target);
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
          connection?.touch(2, pendingPoint);
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
    if (target) connection?.touch(1, target);
    lastPoint = undefined;
    pointer = undefined;
    if (canvas.hasPointerCapture(event.pointerId))
      canvas.releasePointerCapture(event.pointerId);
  }
  function navigate(key: 3 | 4 | 187) {
    if (controlled && !gesture && pointer === undefined)
      connection?.navigate(key);
  }
  function swipe(direction: "up" | "down" | "left" | "right") {
    if (!controlled || gesture || pointer !== undefined) return;
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
    function step(now: number) {
      if (version !== gestureVersion || !controlled) return;
      const progress = Math.min(1, (now - started) / 280);
      lastPoint = position(progress);
      connection?.touch(progress === 1 ? 1 : 2, lastPoint);
      if (progress === 1) {
        lastPoint = undefined;
        gesture = false;
      } else requestAnimationFrame(step);
    }
    requestAnimationFrame(step);
  }
  function keyboard(event: KeyboardEvent) {
    if (!controlled) return;
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
    supported = window.isSecureContext && "VideoDecoder" in window;
    canFullscreen = document.fullscreenEnabled;
    const visibility = () => {
      if (document.hidden) cancelTouch();
    };
    const fullscreenChanged = () => {
      isFullscreen = document.fullscreenElement === stage;
    };
    document.addEventListener("visibilitychange", visibility);
    document.addEventListener("fullscreenchange", fullscreenChanged);
    window.addEventListener("blur", cancelTouch);
    return () => {
      document.removeEventListener("visibilitychange", visibility);
      document.removeEventListener("fullscreenchange", fullscreenChanged);
      window.removeEventListener("blur", cancelTouch);
      stop();
    };
  });
  $effect(() => {
    if (!online) stop();
  });
</script>

{#snippet swipeControls()}
  <div class="live-swipes" role="group" aria-label="手机滑动">
    <Button
      variant="outline"
      size="icon"
      aria-label="上滑"
      title="上滑"
      disabled={!controlled || gesture || pointer !== undefined}
      onclick={() => swipe("up")}><ArrowUp size={20} /></Button
    >
    <Button
      variant="outline"
      size="icon"
      aria-label="下滑"
      title="下滑"
      disabled={!controlled || gesture || pointer !== undefined}
      onclick={() => swipe("down")}><ArrowDown size={20} /></Button
    >
    <Button
      variant="outline"
      size="icon"
      aria-label="左滑"
      title="左滑"
      disabled={!controlled || gesture || pointer !== undefined}
      onclick={() => swipe("left")}><ArrowLeft size={20} /></Button
    >
    <Button
      variant="outline"
      size="icon"
      aria-label="右滑"
      title="右滑"
      disabled={!controlled || gesture || pointer !== undefined}
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
          ? "操作中"
          : "实时画面"
        : viewState === "connecting"
          ? "连接中"
          : "未投屏"}</span
    >
  </div>
</div>
<div class="live-stage" bind:this={stage}>
  <div class="device-screen-body">
    <div class="live-device" class:visible={viewState === "live"}>
      {#if viewState === "live"}
        <nav class="live-navigation" aria-label="手机导航键">
          <Button
            variant="ghost"
            size="icon"
            aria-label="返回"
            title="返回"
            disabled={!controlled || gesture || pointer !== undefined}
            onclick={() => navigate(4)}><ArrowLeft size={20} /></Button
          >
          <Button
            variant="ghost"
            size="icon"
            aria-label="主页"
            title="主页"
            disabled={!controlled || gesture || pointer !== undefined}
            onclick={() => navigate(3)}><Circle size={20} /></Button
          >
          <Button
            variant="ghost"
            size="icon"
            aria-label="最近任务"
            title="最近任务"
            disabled={!controlled || gesture || pointer !== undefined}
            onclick={() => navigate(187)}><PanelsTopLeft size={20} /></Button
          >
          {#if canFullscreen}<Button
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
        class:controllable={controlled}
        aria-label={`${name}实时屏幕${controlled ? "，可点按拖动，方向键滑动" : "，仅观看"}`}
        tabindex={controlled ? 0 : -1}
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
          src={picture}
          alt={`${name}最近一次截图`}
        />
      {:else}<div class="device-screen-placeholder">
          <Monitor size={32} strokeWidth={1.4} /><strong
            >{viewState === "error" ? "投屏已中断" : "查看实时画面"}</strong
          ><span
            >{online
              ? "解锁手机后，点击标题旁的开启投屏"
              : "手机连接后可开启投屏"}</span
          >
        </div>{/if}
    {/if}
  </div>
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
{#if error}<p class="error live-error" role="alert">{error}</p>{/if}
{#if !supported}<p class="hint">
    实时投屏需支持视频解码的浏览器。本机请使用 localhost 或
    127.0.0.1；其他设备需 HTTPS。
  </p>
{/if}
<p class="hint live-footnote">
  切换页签或窗口保留投屏与操作模式，并松开触点；离开设备页后停止投屏。
</p>
