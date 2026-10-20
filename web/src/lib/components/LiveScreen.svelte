<script lang="ts">
  import { onMount } from "svelte";
  import {
    Camera,
    LoaderCircle,
    Maximize,
    Monitor,
    Square,
  } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { startMirror } from "$lib/mirror";

  let {
    id,
    name,
    online,
    picture,
    capturing,
    onCapture,
  }: {
    id: string;
    name: string;
    online: boolean;
    picture?: string;
    capturing: boolean;
    onCapture: () => void;
  } = $props();
  let canvas: HTMLCanvasElement;
  let viewState = $state<"idle" | "connecting" | "live" | "error">("idle");
  let error = $state("");
  let dimensions = $state("");
  let supported = $state(false);
  let canFullscreen = $state(false);
  let stopStream: (() => void) | undefined;

  function stop() {
    stopStream?.();
    stopStream = undefined;
    viewState = "idle";
    dimensions = "";
  }
  function start() {
    stop();
    error = "";
    viewState = "connecting";
    stopStream = startMirror(
      id,
      canvas,
      (width, height) => {
        viewState = "live";
        dimensions = `${width} × ${height}`;
      },
      (message) => {
        viewState = "error";
        error = message;
        stopStream = undefined;
      },
    );
  }
  onMount(() => {
    supported = window.isSecureContext && "VideoDecoder" in window;
    canFullscreen = document.fullscreenEnabled;
    const onVisibility = () => {
      if (document.hidden) stop();
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      stop();
    };
  });
  $effect(() => {
    if (!online) stop();
  });
</script>

<div class="device-row">
  <h2>手机屏幕</h2>
  <span
    class="device-status"
    class:offline={viewState !== "live"}
    role="status"
  >
    <span class="status-dot" aria-hidden="true"></span>
    {viewState === "live"
      ? "实时画面"
      : viewState === "connecting"
        ? "连接中"
        : "未投屏"}
  </span>
</div>
<div class="device-screen-body">
  <canvas
    bind:this={canvas}
    class="device-screenshot live-canvas"
    class:visible={viewState === "live"}
    aria-label={`${name}实时屏幕，仅供观看`}
  ></canvas>
  {#if viewState === "connecting"}
    <div class="device-screen-placeholder">
      <LoaderCircle size={32} class="spin" /><strong>正在连接手机…</strong><span
        >首次连接需要几秒钟</span
      >
    </div>
  {:else if viewState !== "live"}
    {#if picture && viewState !== "error"}
      <img
        class="device-screenshot"
        src={picture}
        alt={`${name}最近一次截图`}
      />
    {:else}
      <div class="device-screen-placeholder">
        <Monitor size={32} strokeWidth={1.4} /><strong
          >{viewState === "error" ? "投屏已中断" : "查看实时画面"}</strong
        ><span
          >{online
            ? "解锁手机后，点击下方开启投屏"
            : "手机连接后可开启投屏"}</span
        >
      </div>
    {/if}
  {/if}
</div>
{#if error}<p class="error live-error" role="alert">{error}</p>{/if}
{#if !supported}
  <p class="hint">
    实时投屏需使用支持视频解码的浏览器。请在这台 Mac 上打开 localhost 或
    127.0.0.1；其他设备需 HTTPS。
  </p>
{:else}
  <p class="hint">
    {viewState === "live"
      ? `${dimensions} · 仅观看，不操作手机`
      : picture
        ? "当前为静态截图，开启投屏后实时更新。"
        : "实时观看手机画面，可与任务同时使用。"}
  </p>
{/if}
<div class="live-actions">
  {#if viewState === "live" || viewState === "connecting"}
    <Button variant="outline" onclick={stop}
      ><Square size={15} />{viewState === "connecting"
        ? "取消连接"
        : "停止投屏"}</Button
    >
  {:else}
    <Button disabled={!online || !supported} onclick={start}
      ><Monitor size={15} />{viewState === "error"
        ? "重新投屏"
        : "开启投屏"}</Button
    >
  {/if}
  {#if viewState === "live" && canFullscreen}
    <Button
      variant="outline"
      title="按 Esc 退出全屏"
      onclick={() =>
        canvas.requestFullscreen().catch(() => {
          error = "暂时无法全屏，请在浏览器中重试";
        })}
    >
      <Maximize size={15} />全屏观看
    </Button>
  {/if}
  <Button variant="outline" disabled={!online || capturing} onclick={onCapture}>
    {#if capturing}<LoaderCircle size={15} class="spin" />获取中…{:else}<Camera
        size={15}
      />原图截图{/if}
  </Button>
</div>
<p class="hint live-footnote">
  离开设备页或将页面切到后台后停止投屏；原图截图仍保留完整分辨率。
</p>
