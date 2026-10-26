<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Device } from "$lib/api";
  import type { CanvasNode, CoordinateProfile } from "$lib/workflow-editor";
  let {
    node,
    devices,
    initialDevice,
    onSave,
    onClose,
    conditionIndex = null,
  }: {
    node: CanvasNode;
    devices: Device[];
    initialDevice: string;
    onSave: (profile: CoordinateProfile) => void;
    onClose: () => void;
    conditionIndex?: number | null;
  } = $props();
  let dialog: HTMLDialogElement;
  let device = $state("");
  let image = $state("");
  let screen = $state<{
    width: number;
    height: number;
    display: string;
  } | null>(null);
  let busy = $state(false);
  let error = $state("");
  let start = $state<[number, number] | null>(null);
  let point = $state({
    x: 0.5,
    y: 0.8,
    to_x: 0.5,
    to_y: 0.3,
  });
  let marked = $state(false);
  const area = $derived(conditionIndex !== null);
  const swipe = $derived(node.data.kind === "swipe" || area);
  async function capture() {
    busy = true;
    error = "";
    image = "";
    screen = null;
    marked = false;
    try {
      const result = await api<{
        image: string;
        screen: { width: number; height: number; display: string };
      }>("/editor/observe/" + encodeURIComponent(device));
      image = result.image;
      screen = {
        width: result.screen.width,
        height: result.screen.height,
        display: result.screen.display,
      };
      const previous = (
        conditionIndex !== null
          ? node.data.config.conditions?.[conditionIndex]?.regions
          : node.data.config.profiles
      )?.find(
        (item) =>
          item.device_id === device &&
          item.width === screen?.width &&
          item.height === screen?.height &&
          item.display === screen?.display,
      );
      if (previous) {
        point = {
          x: previous.x,
          y: previous.y,
          to_x: previous.to_x,
          to_y: previous.to_y,
        };
        marked = true;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : "取图失败";
    } finally {
      busy = false;
    }
  }
  function position(event: PointerEvent): [number, number] {
    const rect =
      event.currentTarget instanceof HTMLElement
        ? event.currentTarget.getBoundingClientRect()
        : new DOMRect();
    return [
      Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)),
      Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height)),
    ];
  }
  function down(event: PointerEvent) {
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    start = position(event);
    point = {
      ...point,
      x: start[0],
      y: start[1],
      to_x: start[0],
      to_y: start[1],
    };
    marked = true;
  }
  function move(event: PointerEvent) {
    if (start && swipe) {
      const p = position(event);
      point = { ...point, to_x: p[0], to_y: p[1] };
    }
  }
  function up(event: PointerEvent) {
    move(event);
    start = null;
  }
  onMount(() => {
    device = initialDevice || devices.find((item) => item.online)?.id || "";
    point = {
      x: node.data.config.x ?? node.data.config.from_x ?? 0.5,
      y: node.data.config.y ?? node.data.config.from_y ?? 0.8,
      to_x: node.data.config.to_x ?? 0.5,
      to_y: node.data.config.to_y ?? 0.3,
    };
    dialog.showModal();
    if (device) void capture();
  });
</script>

<dialog bind:this={dialog} onclose={onClose} oncancel={onClose}>
  <header>
    <h2>
      {area ? "框选文字识别区域" : swipe ? "标注滑动路线" : "标注点击位置"}
    </h2>
    <button onclick={onClose}>关闭</button>
  </header>
  <p>
    在手机截图上{area
      ? "拖动框选区域"
      : swipe
        ? "从起点拖到终点"
        : "点击目标"}。标注不会操作手机。
  </p>
  <div class="picker-tools">
    <label
      >目标手机<select bind:value={device} onchange={capture}
        >{#each devices as item}<option value={item.id} disabled={!item.online}
            >{item.name}{item.online ? "" : "（离线）"}</option
          >{/each}</select
      ></label
    ><button disabled={busy || !device} onclick={capture}
      >{busy ? "正在取图…" : "重新取图"}</button
    >
  </div>
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#if image && screen}
    <div class="picker-body">
      <button
        class="image-surface"
        aria-label={swipe ? "在截图上拖动标注滑动路线" : "在截图上点选位置"}
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={() => (start = null)}
      >
        <img src={image} alt="用于位置标注的手机截图" draggable="false" />
        {#if marked}<svg
            viewBox="0 0 100 100"
            preserveAspectRatio="none"
            aria-hidden="true"
            >{#if area}<rect
                x={Math.min(point.x, point.to_x) * 100}
                y={Math.min(point.y, point.to_y) * 100}
                width={Math.abs(point.to_x - point.x) * 100}
                height={Math.abs(point.to_y - point.y) * 100}
                fill="none"
                stroke="var(--accent)"
                stroke-width="0.6"
              />{/if}{#if swipe && !area}<line
                x1={point.x * 100}
                y1={point.y * 100}
                x2={point.to_x * 100}
                y2={point.to_y * 100}
              />{/if}<circle
              cx={point.x * 100}
              cy={point.y * 100}
              r="1.2"
            />{#if swipe}<circle
                cx={point.to_x * 100}
                cy={point.to_y * 100}
                r="1.2"
              />{/if}</svg
          >{/if}
      </button>
      <div class="numeric">
        <p>画面：{screen.width} × {screen.height}</p>
        {#each swipe ? ["x", "y", "to_x", "to_y"] : ["x", "y"] as key}
          <label
            >{(
              {
                x: "起点横向",
                y: "起点纵向",
                to_x: "终点横向",
                to_y: "终点纵向",
              } as Record<string, string>
            )[key]}（%）<input
              type="number"
              min="0"
              max="100"
              step="0.1"
              value={Math.round(point[key as keyof typeof point] * 1000) / 10}
              oninput={(event) => {
                if (Number.isFinite(event.currentTarget.valueAsNumber)) {
                  point = {
                    ...point,
                    [key]: event.currentTarget.valueAsNumber / 100,
                  };
                  marked = true;
                }
              }}
            /></label
          >
        {/each}
        <p>
          这份标注只用于当前手机及屏幕形态。横竖屏、折叠屏切换后需分别标注。
        </p>
        <button
          class="save"
          disabled={!marked ||
            Object.values(point).some((n) => n < 0 || n > 1) ||
            (area && (point.x === point.to_x || point.y === point.to_y))}
          onclick={() => {
            if (screen)
              onSave({
                device_id: device,
                ...screen,
                ...(area
                  ? {
                      x: Math.min(point.x, point.to_x),
                      y: Math.min(point.y, point.to_y),
                      to_x: Math.max(point.x, point.to_x),
                      to_y: Math.max(point.y, point.to_y),
                    }
                  : point),
              });
          }}>保存这份标注</button
        >
      </div>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    width: min(780px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    margin: auto;
    padding: 22px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: var(--panel);
    color: var(--text);
    overflow: auto;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }
  h2 {
    font-size: 20px;
  }
  p {
    font-size: 13px;
    line-height: 1.7;
    color: var(--dim);
    margin: 12px 0;
  }
  button {
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 9px 12px;
    min-height: 40px;
  }
  .picker-tools {
    display: flex;
    gap: 12px;
    align-items: end;
  }
  .picker-tools label {
    flex: 1;
    margin: 8px 0 0;
  }
  .picker-tools button {
    margin-bottom: 0;
  }
  .picker-body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 235px;
    gap: 24px;
    margin-top: 20px;
    align-items: start;
  }
  .image-surface {
    position: relative;
    padding: 0;
    line-height: 0;
    cursor: crosshair;
    touch-action: none;
    overflow: hidden;
    max-width: 100%;
    justify-self: center;
  }
  img {
    display: block;
    max-width: 100%;
    max-height: 60dvh;
    width: auto;
  }
  svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  line {
    stroke: var(--accent);
    stroke-width: 0.6;
  }
  circle {
    fill: var(--accent);
    stroke: white;
    stroke-width: 0.3;
  }
  .numeric label {
    font-size: 12px;
    margin: 12px 0;
  }
  .save {
    background: var(--accent);
    color: var(--primary-ink);
    width: 100%;
  }
  .error {
    color: var(--red);
  }
  @media (max-width: 600px) {
    .picker-body {
      grid-template-columns: 1fr;
    }
    .numeric {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 10px;
    }
    .numeric p,
    .numeric .save {
      grid-column: 1/-1;
    }
  }
</style>
