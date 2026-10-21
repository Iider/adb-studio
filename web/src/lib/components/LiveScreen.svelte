<script lang="ts">
  import { onMount } from "svelte";
  import {
    Camera,
    LoaderCircle,
    Maximize,
    Monitor,
    Square,
    MousePointer2,
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
  import { api, type Run } from "$lib/api";

  let {
    id,
    name,
    online,
    picture,
    capturing,
    run,
    onCapture,
  }: {
    id: string;
    name: string;
    online: boolean;
    picture?: string;
    capturing: boolean;
    run?: Run;
    onCapture: () => void;
  } = $props();
  let canvas: HTMLCanvasElement;
  let stage: HTMLDivElement;
  let viewState = $state<"idle" | "connecting" | "live" | "error">("idle");
  let error = $state("");
  let dimensions = $state("");
  let supported = $state(false);
  let canFullscreen = $state(false);
  let controlled = $state(false);
  let requesting = $state(false);
  let waitingPause = $state(false);
  let gesture = $state(false);
  let pointer = $state<number | undefined>();
  let lastPoint: ScreenPoint | undefined;
  let pendingPoint: ScreenPoint | undefined;
  let moveFrame = 0;
  let gestureVersion = 0;
  let connection: ReturnType<typeof startMirror> | undefined;

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
  function endControl() {
    waitingPause = false;
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
  }
  function start() {
    stop();
    error = "";
    viewState = "connecting";
    connection = startMirror(
      id,
      canvas,
      (width, height) => {
        viewState = "live";
        dimensions = `${width} × ${height}`;
      },
      (message) => {
        viewState = "error";
        error = message;
        connection = undefined;
      },
      (enabled, message) => {
        controlled = enabled;
        requesting = false;
        if (!enabled) {
          cancelTouch();
          waitingPause = false;
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
          cancelTouch();
          endControl();
          error = "画面尺寸已变化，已结束操作，请核对画面后重新开启";
        }
      },
    );
  }
  async function requestControl() {
    error = "";
    requesting = true;
    if (run && run.state !== "paused") {
      waitingPause = true;
      try {
        await api(`/runs/${run.id}/pause`, "POST");
      } catch (e) {
        waitingPause = false;
        requesting = false;
        error = e instanceof Error ? e.message : "暂停失败";
      }
    } else connection?.setControl(true);
  }
  $effect(() => {
    if (
      waitingPause &&
      (!run || (run.state === "paused" && !run.pause_requested))
    ) {
      waitingPause = false;
      connection?.setControl(true);
    }
  });
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
      if (document.hidden) stop();
    };
    const blur = () => {
      if (controlled || requesting) endControl();
    };
    document.addEventListener("visibilitychange", visibility);
    window.addEventListener("blur", blur);
    return () => {
      document.removeEventListener("visibilitychange", visibility);
      window.removeEventListener("blur", blur);
      stop();
    };
  });
  $effect(() => {
    if (!online) stop();
  });
</script>

<div class="device-row">
  <h2>手机屏幕</h2>
  <span class="device-status" class:offline={viewState !== "live"} role="status"
    ><span class="status-dot" aria-hidden="true"></span>{viewState === "live"
      ? controlled
        ? "操作中"
        : "实时画面"
      : viewState === "connecting"
        ? "连接中"
        : "未投屏"}</span
  >
</div>
<div class="live-stage" class:ready={viewState === "live"} bind:this={stage}>
  <div class="device-screen-body">
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
              ? "解锁手机后，点击下方开启投屏"
              : "手机连接后可开启投屏"}</span
          >
        </div>{/if}
    {/if}
  </div>
  {#if viewState === "live"}
    <div class="live-control-area">
      <div class="live-control-mode">
        {#if controlled}<Button variant="outline" onclick={endControl}
            ><Square size={15} />结束操作</Button
          >
        {:else if requesting}<Button variant="outline" onclick={endControl}
            ><LoaderCircle class="spin" size={15} />{waitingPause
              ? "正在暂停…取消等待"
              : "正在开启…取消"}</Button
          >
        {:else}<Button onclick={requestControl}
            ><MousePointer2 size={15} />{run && run.state !== "paused"
              ? "暂停并操作"
              : "操作手机"}</Button
          >{/if}
        <Button
          variant="outline"
          class="fullscreen-exit"
          onclick={() => document.exitFullscreen()}
          ><Minimize size={15} />退出全屏</Button
        >
      </div>
      <nav class="live-navigation" aria-label="手机导航键">
        <Button
          variant="outline"
          disabled={!controlled || gesture || pointer !== undefined}
          onclick={() => navigate(4)}><ArrowLeft size={17} />返回</Button
        >
        <Button
          variant="outline"
          disabled={!controlled || gesture || pointer !== undefined}
          onclick={() => navigate(3)}><Circle size={16} />主页</Button
        >
        <Button
          variant="outline"
          disabled={!controlled || gesture || pointer !== undefined}
          onclick={() => navigate(187)}
          ><PanelsTopLeft size={17} />最近任务</Button
        >
      </nav>
      <div class="live-swipes" role="group" aria-label="手机滑动">
        <Button
          variant="outline"
          disabled={!controlled || gesture || pointer !== undefined}
          onclick={() => swipe("up")}><ArrowUp size={14} />上滑</Button
        >
        <Button
          variant="outline"
          disabled={!controlled || gesture || pointer !== undefined}
          onclick={() => swipe("down")}><ArrowDown size={14} />下滑</Button
        >
        <Button
          variant="outline"
          disabled={!controlled || gesture || pointer !== undefined}
          onclick={() => swipe("left")}><ArrowLeft size={14} />左滑</Button
        >
        <Button
          variant="outline"
          disabled={!controlled || gesture || pointer !== undefined}
          onclick={() => swipe("right")}><ArrowRight size={14} />右滑</Button
        >
      </div>
      <p class="hint live-operation-hint">
        {controlled
          ? "点按画面 · 按住拖动 · 结束后任务仍保持暂停"
          : run
            ? "操作前会等待任务暂停；结束后由你决定何时继续。"
            : "仅观看，开启操作后可点按、拖动和使用导航键。"}
      </p>
    </div>
  {/if}
</div>
{#if error}<p class="error live-error" role="alert">{error}</p>{/if}
{#if !supported}<p class="hint">
    实时投屏需支持视频解码的浏览器。本机请使用 localhost 或
    127.0.0.1；其他设备需 HTTPS。
  </p>
{:else if viewState === "live"}<p class="hint">
    {dimensions} · {controlled ? "操作手机" : "仅观看"}
  </p>{/if}
<div class="live-actions">
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
  {#if viewState === "live" && canFullscreen}<Button
      variant="outline"
      title="按 Esc 退出全屏"
      onclick={() =>
        stage.requestFullscreen().catch(() => {
          error = "暂时无法全屏，请在浏览器中重试";
        })}><Maximize size={15} />全屏观看</Button
    >{/if}
  <Button variant="outline" disabled={!online || capturing} onclick={onCapture}
    >{#if capturing}<LoaderCircle size={15} class="spin" />获取中…{:else}<Camera
        size={15}
      />原图截图{/if}</Button
  >
</div>
<p class="hint live-footnote">
  离开设备页或切到后台后停止投屏；切换窗口后结束手机操作。
</p>
