<script lang="ts">
  import { onDestroy } from "svelte";
  import { LoaderCircle } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { api, type Device, type VisionInspection } from "$lib/api";

  let { devices, connected }: { devices: Device[]; connected: boolean } =
    $props();
  let source = $state("phone");
  let deviceId = $state("");
  let file = $state<File>();
  let condition = $state("");
  let target = $state("");
  let readNumber = $state(false);
  let pending = $state(false);
  let error = $state("");
  let result = $state<VisionInspection | null>(null);
  let controller: AbortController | undefined;
  $effect(() => {
    if (!devices.some((d) => d.id === deviceId))
      deviceId = (devices.find((d) => d.online) || devices[0])?.id || "";
  });
  onDestroy(() => controller?.abort());

  async function check() {
    if (pending) return;
    error = "";
    result = null;
    if (source === "file" && (!file || file.size > 8 * 1024 * 1024)) {
      error = "请选择不超过 8 MiB 的 PNG 图片。";
      return;
    }
    pending = true;
    controller = new AbortController();
    try {
      const image =
        file && source === "file"
          ? await new Promise<string>((resolve, reject) => {
              const reader = new FileReader();
              reader.onload = () =>
                resolve(String(reader.result).split(",")[1]);
              reader.onerror = () => reject(new Error("图片读取失败"));
              reader.readAsDataURL(file!);
            })
          : undefined;
      result = await api<VisionInspection>(
        "/recognition/check",
        "POST",
        {
          device_id: source === "phone" ? deviceId : null,
          image_base64: image || null,
          query: {
            condition,
            target: target.trim() || null,
            read_number: readNumber,
          },
        },
        controller.signal,
      );
    } catch (e) {
      error = e instanceof Error ? e.message : "识别检查失败";
    } finally {
      pending = false;
    }
  }
</script>

<form
  class="panel vision-inspector"
  onsubmit={(e) => {
    e.preventDefault();
    void check();
  }}
  aria-busy={pending}
>
  <h2>识别检查</h2>
  <p class="hint">
    只看图，不操作手机。截图发送到已保存的模型服务，每次最多请求 2 次。
  </p>
  <fieldset disabled={pending} class="vision-fields">
    <label
      >图片来源<select bind:value={source}>
        <option value="phone">手机当前画面</option><option value="file"
          >本地 PNG</option
        >
      </select></label
    >
    {#if source === "phone"}
      <label
        >手机<select bind:value={deviceId} disabled={!devices.length}>
          {#each devices as device}<option value={device.id}
              >{device.name} · {device.online ? "在线" : "离线"}</option
            >{/each}
        </select></label
      >
    {:else}
      <label
        >PNG 图片<input
          type="file"
          accept="image/png,.png"
          required
          onchange={(e) => {
            file = e.currentTarget.files?.[0];
          }}
        /></label
      >
    {/if}
    <label
      >判断条件<textarea
        required
        maxlength="600"
        rows="3"
        bind:value={condition}
        placeholder="例如：当前是手机设置首页，并且能看到 WLAN 入口。"
      ></textarea></label
    >
    <label
      >定位目标（可不填）<input
        maxlength="150"
        bind:value={target}
        placeholder="例如：WLAN 入口的中心"
      /></label
    >
    <label class="vision-checkbox"
      ><input
        type="checkbox"
        bind:checked={readNumber}
      />同时读取图中的整数</label
    >
    <Button
      type="submit"
      disabled={!connected ||
        (source === "phone" && !devices.find((d) => d.id === deviceId)?.online)}
    >
      {#if pending}<LoaderCircle
          size={16}
          class="spin"
        />识别中…{:else}检查画面{/if}
    </Button>
  </fieldset>
  <p class="hint vision-status" role="status">
    {pending ? "正在取图和识别，请稍候。" : error}
  </p>
  {#if result}
    <div class="vision-result" aria-live="polite">
      <h3>
        {result.error
          ? "检查失败"
          : result.result?.choice === "confirmed"
            ? "已确认"
            : "无法确认"}
      </h3>
      <p>{result.error || result.result?.reason}</p>
      <p class="hint">
        截图 {result.capture_ms} 毫秒 · 模型 {result.model_ms} 毫秒 · {result.calls}
        次请求
      </p>
      <p class="hint">
        模型：{result.model.model} · 判断条件：{result.query.condition}
      </p>
      {#if result.result?.target}<p>
          位置：{result.result.target.join("，")}
        </p>{/if}
      {#if result.result?.coin != null}<p>数字：{result.result.coin}</p>{/if}
      <div class="vision-picture">
        <img
          src={`/api/recognition/samples/${result.image}`}
          alt="本次识别使用的截图"
        />
        {#if result.result?.choice === "confirmed" && result.result.target}
          <span
            class="vision-target"
            style:left={`${(result.result.target[0] / result.size[0]) * 100}%`}
            style:top={`${(result.result.target[1] / result.size[1]) * 100}%`}
            aria-label="模型定位位置"
          ></span>
        {/if}
      </div>
      <p class="hint">样本：{result.image}。定位仅供检查，不会触发点击。</p>
    </div>
  {:else}
    <div class="vision-result empty">检查后在这里查看判断依据与截图。</div>
  {/if}
</form>
