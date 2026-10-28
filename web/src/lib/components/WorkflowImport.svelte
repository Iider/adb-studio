<script lang="ts">
  import { tick } from "svelte";
  import { goto } from "$app/navigation";
  import { FileUp, LoaderCircle, X } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { apiResponse } from "$lib/api";

  interface Preview {
    name: string;
    kind: string;
    steps: number;
    images: number;
    devices: { id: string; name: string; status: string }[];
    warnings: string[];
  }
  interface Result {
    workflow_id: string | null;
    draft_id: string | null;
    name: string;
    removed: boolean;
  }
  let {
    open,
    onclose,
    onimport,
  }: { open: boolean; onclose: () => void; onimport: () => void } = $props();
  let dialog = $state<HTMLDialogElement>();
  let picker = $state<HTMLInputElement>();
  let filename = $state("");
  let name = $state("");
  let preview = $state<Preview | null>(null);
  let result = $state<Result | null>(null);
  let error = $state("");
  let checking = $state(false);
  let saving = $state(false);
  let locked = $state(false);
  let file: unknown = null;
  let operation = "";
  let submission: { operation_id: string; file: unknown; name: string } | null =
    null;
  let controller: AbortController | null = null;
  let generation = 0;
  let returnFocus: HTMLElement | null = null;

  $effect(() => {
    if (!open) return;
    returnFocus =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    filename = "";
    name = "";
    preview = null;
    result = null;
    error = "";
    checking = false;
    saving = false;
    locked = false;
    submission = null;
    void tick().then(() => {
      if (open && dialog?.isConnected && !dialog.open) dialog.showModal();
    });
    return () => {
      controller?.abort();
      generation++;
    };
  });

  function closed() {
    controller?.abort();
    generation++;
    onclose();
    returnFocus?.focus();
  }
  async function choose(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const selected = input.files?.[0];
    input.value = "";
    if (!selected) return;
    controller?.abort();
    controller = new AbortController();
    const signal = controller.signal;
    const current = ++generation;
    preview = null;
    result = null;
    error = "";
    submission = null;
    locked = false;
    filename = selected.name;
    checking = true;
    try {
      if (selected.size > 32 * 1024 * 1024)
        throw new Error("文件不能超过 32 MiB");
      const content = await selected.text();
      if (current !== generation) return;
      try {
        file = JSON.parse(content);
      } catch {
        throw new Error("无法读取文件，请选择工作台导出的 JSON 文件");
      }
      operation = crypto.randomUUID();
      const response = await apiResponse(
        "/workflow-import/preview",
        "POST",
        { operation_id: operation, file },
        signal,
      );
      const summary = (await response.json()) as Preview;
      if (current !== generation) return;
      preview = summary;
      name = summary.name;
    } catch (cause) {
      if (current === generation && !signal.aborted)
        error = cause instanceof Error ? cause.message : "文件检查失败，请重试";
    } finally {
      if (current === generation) checking = false;
    }
  }

  async function save() {
    if (!preview || saving) return;
    submission ??= { operation_id: operation, file, name };
    locked = true;
    saving = true;
    error = "";
    try {
      const response = await apiResponse(
        "/workflow-import",
        "POST",
        submission,
      );
      result = (await response.json()) as Result;
      onimport();
      if (!result.removed && !result.workflow_id && result.draft_id) {
        await goto(`/editor?draft=${encodeURIComponent(result.draft_id)}`);
      }
    } catch (cause) {
      error =
        cause instanceof Error ? cause.message : "导入结果尚未确认，请重试";
      if (
        cause instanceof Error &&
        "status" in cause &&
        [400, 413, 422].includes(Number(cause.status))
      ) {
        locked = false;
        submission = null;
        operation = crypto.randomUUID();
      }
    } finally {
      saving = false;
    }
  }

  const editHref = $derived(
    result?.draft_id
      ? `/editor?draft=${encodeURIComponent(result.draft_id)}`
      : "/editor",
  );
</script>

<dialog
  bind:this={dialog}
  class="import-dialog"
  aria-labelledby="workflow-import-title"
  onclose={closed}
  oncancel={(event) => {
    if (saving) event.preventDefault();
  }}
>
  <header>
    <h2 id="workflow-import-title">导入工作流文件</h2>
    <Button
      variant="ghost"
      size="icon"
      aria-label="关闭导入"
      disabled={saving}
      onclick={() => dialog?.close()}><X size={18} aria-hidden="true" /></Button
    >
  </header>
  {#if result}
    <p role="status">
      {result.removed
        ? "此前已完成导入，该副本后来已被删除。"
        : `已导入：${result.name}`}
    </p>
    <footer>
      <Button variant="outline" onclick={() => dialog?.close()}>完成</Button
      >{#if !result.removed}<Button href={editHref}>编辑工作流</Button>{/if}
    </footer>
  {:else}
    <p class="intro">导入为独立副本，保留已有任务，不自动运行。</p>
    <input
      class="visually-hidden"
      type="file"
      accept=".json,application/json"
      bind:this={picker}
      onchange={choose}
      tabindex="-1"
      aria-label="选择工作流文件"
    />
    <Button variant="outline" disabled={saving} onclick={() => picker?.click()}
      ><FileUp size={16} aria-hidden="true" />{filename
        ? "重新选择文件"
        : "选择文件"}</Button
    >
    {#if filename}<p class="filename">{filename}</p>{/if}
    {#if checking}<p class="progress" role="status">
        <LoaderCircle class="spin" size={16} aria-hidden="true" />正在检查文件…
      </p>{/if}
    {#if preview}
      <label for="workflow-import-name">工作流名称</label>
      <input id="workflow-import-name" bind:value={name} disabled={locked} />
      <dl>
        <div>
          <dt>内容</dt>
          <dd>{preview.kind}</dd>
        </div>
        <div>
          <dt>步骤</dt>
          <dd>{preview.steps} 步</dd>
        </div>
        <div>
          <dt>图片样本</dt>
          <dd>{preview.images} 张</dd>
        </div>
      </dl>
      <div class="phones">
        <span>适用手机</span>{#if !preview.devices.length}<p>
            不限定手机
          </p>{:else}{#each preview.devices as phone (phone.id)}<p>
              {phone.name} · {phone.status}
            </p>{/each}{/if}
      </div>
      {#each preview.warnings as warning}<p class="notice">{warning}</p>{/each}
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <footer>
      <Button
        variant="outline"
        disabled={saving}
        onclick={() => dialog?.close()}>取消</Button
      ><Button
        disabled={!preview || checking || saving || !name.trim()}
        onclick={save}
        >{saving ? "正在导入…" : locked ? "重试导入" : "导入副本"}</Button
      >
    </footer>
  {/if}
</dialog>

<style>
  .import-dialog {
    width: min(520px, calc(100vw - 36px));
    max-height: calc(100dvh - 36px);
    overflow-y: auto;
    margin: auto;
    padding: 22px;
    border: 1px solid var(--line);
    border-radius: 16px;
    color: var(--text);
    background: var(--panel);
  }
  header,
  footer,
  .progress {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  header {
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  p {
    margin: 14px 0;
    line-height: 1.65;
    overflow-wrap: anywhere;
  }
  .intro,
  .filename {
    color: var(--dim);
    font-size: 13px;
  }
  label {
    display: block;
    margin-top: 18px;
    font-size: 13px;
  }
  input:not([type="file"]) {
    width: 100%;
    margin: 8px 0 0;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    margin: 18px 0;
  }
  dt,
  .phones > span {
    color: var(--dim);
    font-size: 12px;
  }
  dl > div {
    display: grid;
    gap: 6px;
  }
  dd {
    margin: 8px 0 0;
    font-size: 13px;
  }
  .phones p {
    margin: 6px 0;
    font-size: 13px;
  }
  .notice {
    padding: 10px 12px;
    background: var(--accentsoft);
    border-radius: 8px;
    font-size: 13px;
  }
  .error {
    color: var(--red);
  }
  footer {
    justify-content: flex-end;
    margin-top: 22px;
  }
  .progress :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .progress :global(.spin) {
      animation: none;
    }
  }
  @media (max-width: 660px) {
    .import-dialog :global([data-slot="button"]) {
      min-height: 44px;
    }
  }
</style>
