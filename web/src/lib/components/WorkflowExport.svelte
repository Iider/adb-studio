<script lang="ts">
  import { tick } from "svelte";
  import { Download, LoaderCircle, X } from "@lucide/svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { apiResponse } from "$lib/api";
  import { notify } from "$lib/notifications";

  let {
    request,
    onclose,
  }: {
    request: { path: string; name: string } | null;
    onclose: () => void;
  } = $props();
  let dialog = $state<HTMLDialogElement>();
  let pending = $state(false);
  let error = $state("");
  let file = $state<Blob | null>(null);
  let draftState = $state("");
  let mode = $state("");
  let returnFocus: HTMLElement | null = null;

  $effect(() => {
    const source = request;
    if (!source) return;
    const abort = new AbortController();
    pending = true;
    error = "";
    file = null;
    draftState = "";
    mode = "";
    void (async () => {
      await tick();
      if (abort.signal.aborted) return;
      const focused = document.activeElement;
      returnFocus =
        focused instanceof HTMLElement
          ? (focused.closest("details")?.querySelector("summary") ?? focused)
          : null;
      dialog?.showModal();
      try {
        const response = await apiResponse(
          source.path,
          "GET",
          undefined,
          abort.signal,
        );
        const blob = await response.blob();
        if (abort.signal.aborted) return;
        file = blob;
        draftState = response.headers.get("x-workflow-draft-state") ?? "";
        mode = response.headers.get("x-workflow-launch-mode") ?? "";
      } catch (cause) {
        if (!abort.signal.aborted)
          error = cause instanceof Error ? cause.message : "导出失败，请重试";
      } finally {
        if (!abort.signal.aborted) pending = false;
      }
    })();
    return () => abort.abort();
  });

  function close() {
    dialog?.close();
  }

  function closed() {
    onclose();
    returnFocus?.focus();
  }

  function download() {
    if (!file || !request) return;
    const name =
      request.name
        .replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_")
        .replace(/[. ]+$/g, "")
        .slice(0, 80) || "工作流";
    const url = URL.createObjectURL(file);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${name}.workflow.json`;
    document.body.append(anchor);
    anchor.click();
    anchor.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    notify.success("已发起工作流文件下载");
    close();
  }
</script>

<dialog
  bind:this={dialog}
  class="export-dialog"
  onclose={closed}
  aria-labelledby="workflow-export-title"
>
  <header>
    <h2 id="workflow-export-title">导出工作流</h2>
    <Button variant="ghost" size="icon" aria-label="关闭导出" onclick={close}
      ><X size={18} aria-hidden="true" /></Button
    >
  </header>
  <p class="filename">{request?.name}</p>
  {#if pending}
    <p class="progress" role="status">
      <LoaderCircle size={16} class="spin" aria-hidden="true" />正在整理文件…
    </p>
  {:else if error}
    <p class="error" role="alert">{error}</p>
  {:else if file}
    {#if draftState === "modified" || draftState === "incomplete"}
      <p class="notice">文件同时包含已保存任务和待保存编辑；导入后分别保留。</p>
    {:else if draftState === "draft"}
      <p>仅导出当前草稿，包括未完成的步骤；不覆盖已保存任务。</p>
    {:else}
      <p>包含已保存任务{draftState === "saved" ? "与画布布局" : ""}。</p>
    {/if}
    {#if mode === "device_point"}
      <p class="notice">
        点击位置来自目标手机的本机标注，文件不包含该标注；换电脑后需要重新标注。
      </p>
    {/if}
    <p class="sharing">
      文件保留步骤文字、适用手机编号、画布标注及引用的图片样本，请确认后分享。
    </p>
    <p class="size">文件大小：{(file.size / 1024).toFixed(1)} KB</p>
  {/if}
  <footer>
    <Button variant="outline" onclick={close}>取消</Button>
    <Button disabled={pending || !file} onclick={download}
      ><Download size={16} aria-hidden="true" />下载文件</Button
    >
  </footer>
</dialog>

<style>
  .export-dialog {
    width: min(480px, calc(100vw - 36px));
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
  .filename {
    font-weight: 600;
  }
  .sharing,
  .size {
    color: var(--dim);
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
    .export-dialog :global([data-slot="button"]) {
      min-height: 44px;
    }
  }
</style>
