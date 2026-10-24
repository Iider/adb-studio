<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { fly, fade } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { CircleCheck, CircleAlert, X } from "@lucide/svelte";
  import { notifications, notify, TOAST_ENTER_MS } from "$lib/notifications";

  let viewport = $state<HTMLDivElement>();
  let reducedMotion = $state(false);
  const newestId = $derived($notifications.at(-1)?.id);

  function showAboveContent() {
    if (!viewport || !newestId) return;
    const parent =
      document.querySelector("dialog:modal") ||
      document.fullscreenElement ||
      document.body;
    // 通知跟随当前弹窗，避免被模态弹窗的不可操作区域屏蔽。
    viewport.hidePopover();
    if (viewport.parentElement !== parent) parent.append(viewport);
    viewport.showPopover();
  }

  onMount(() => {
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => (reducedMotion = preference.matches);
    update();
    preference.addEventListener("change", update);
    const observer = new MutationObserver(showAboveContent);
    observer.observe(document.body, {
      subtree: true,
      attributes: true,
      attributeFilter: ["open"],
    });
    document.addEventListener("fullscreenchange", showAboveContent);
    return () => {
      preference.removeEventListener("change", update);
      observer.disconnect();
      document.removeEventListener("fullscreenchange", showAboveContent);
    };
  });
  onDestroy(notify.clear);

  $effect(showAboveContent);
</script>

<div
  class="toast-viewport"
  popover="manual"
  bind:this={viewport}
  aria-label="通知"
>
  {#each $notifications as item (item.id)}
    <div
      class="toast"
      class:toast-error={item.kind === "error"}
      role={item.kind === "error" ? "alert" : "status"}
      aria-atomic="true"
      in:fly={{
        y: 18,
        duration: reducedMotion ? 0 : TOAST_ENTER_MS,
        easing: cubicOut,
      }}
      out:fade={{ duration: reducedMotion ? 0 : 160 }}
    >
      {#if item.kind === "error"}<CircleAlert size={19} aria-hidden="true" />
      {:else}<CircleCheck size={19} aria-hidden="true" />{/if}
      <span>{item.message}</span>
      <button
        type="button"
        aria-label="关闭通知"
        onclick={() => notify.dismiss(item.id)}
        ><X size={16} aria-hidden="true" /></button
      >
    </div>
  {/each}
</div>
