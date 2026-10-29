<script lang="ts" module>
  export const stepPickerContext = Symbol("step-picker");
  export type OpenStepPicker = (
    source: string,
    port: string,
    anchor: HTMLElement,
    type?: "source" | "target",
  ) => void;
</script>

<script lang="ts">
  import { tick } from "svelte";
  import { Plus, Search, X } from "@lucide/svelte";
  import { palette, ports, type Kind } from "$lib/workflow-editor";
  import Dropdown, {
    type DropdownItem,
  } from "$lib/components/ui/dropdown/dropdown.svelte";

  let {
    hasNext = false,
    connections = [],
    connection = $bindable(""),
    hint = "插入当前连线，保留原下一步",
    onchoose,
    onclose,
  }: {
    hasNext?: boolean;
    connections?: DropdownItem[];
    connection?: string;
    hint?: string;
    onchoose: (kind: Kind) => void;
    onclose: () => void;
  } = $props();
  let menu: HTMLDivElement;
  let list: HTMLDivElement;
  let searchInput: HTMLInputElement;
  let search = $state("");
  let anchor: HTMLElement | null = null;
  const items = $derived(
    palette.filter((item) =>
      `${item.label}${item.hint}`.includes(search.trim()),
    ),
  );

  export async function open(target: HTMLElement) {
    anchor = target;
    search = "";
    await tick();
    menu.showPopover();
    list.scrollTop = 0;
    const rect = target.getBoundingClientRect();
    const width = menu.getBoundingClientRect().width;
    const height = menu.getBoundingClientRect().height;
    const left =
      rect.right + 12 + width <= window.innerWidth - 12
        ? rect.right + 12
        : rect.left - width - 12;
    menu.style.left = `${Math.max(12, Math.min(left, window.innerWidth - width - 12))}px`;
    menu.style.top = `${Math.max(12, Math.min(rect.top, window.innerHeight - height - 12))}px`;
    searchInput.focus({ preventScroll: true });
  }
  export function close(restoreFocus = false) {
    menu?.hidePopover();
    if (restoreFocus) anchor?.focus({ preventScroll: true });
  }
  function keydown(event: KeyboardEvent) {
    if ((event.target as HTMLElement).closest(".connection-choice")) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
      return;
    }
    const buttons = [
      ...menu.querySelectorAll<HTMLButtonElement>(
        ".step-choice:not(:disabled)",
      ),
    ];
    if (!buttons.length) return;
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next =
        index < 0
          ? event.key === "ArrowDown"
            ? 0
            : buttons.length - 1
          : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) %
            buttons.length;
      buttons[next]?.focus();
    } else if (event.key === "Enter" && event.target === searchInput) {
      event.preventDefault();
      buttons[0]?.click();
    }
  }
</script>

<svelte:window onresize={() => close()} />

<div
  bind:this={menu}
  popover="auto"
  class="step-picker"
  role="menu"
  aria-label="添加步骤"
  tabindex="-1"
  onkeydown={keydown}
  ontoggle={(event) => {
    if (event.newState === "closed") onclose();
  }}
>
  <div class="picker-heading">
    <strong>添加步骤</strong>
    <button type="button" aria-label="关闭节点选择" onclick={() => close(true)}
      ><X size={16} /></button
    >
  </div>
  <label class="picker-search">
    <Search size={15} aria-hidden="true" />
    <input
      bind:this={searchInput}
      type="search"
      aria-label="搜索节点"
      placeholder="搜索节点"
      bind:value={search}
    />
  </label>
  {#if connections.length > 1}
    <div class="connection-choice">
      <Dropdown
        label="选择要插入的连线"
        items={connections}
        bind:value={connection}
      />
    </div>
  {/if}
  {#if hasNext}<p class="insert-hint">{hint}</p>{/if}
  <div class="picker-list" bind:this={list}>
    {#each ["基础动作", "流程控制"] as group}
      {#if items.some((item) => item.group === group)}<h3>{group}</h3>{/if}
      {#each items.filter((item) => item.group === group) as item (item.kind)}
        {@const blocked = hasNext && !ports(item.kind).length}
        <button
          type="button"
          class="step-choice"
          role="menuitem"
          disabled={blocked}
          onclick={() => {
            onchoose(item.kind);
            close();
          }}
        >
          <span
            ><strong>{item.label}</strong><small
              >{blocked ? "此处需要连接后续步骤" : item.hint}</small
            ></span
          >
          <Plus size={16} aria-hidden="true" />
        </button>
      {/each}
    {/each}
    {#if !items.length}<p class="empty">没有符合的节点</p>{/if}
  </div>
</div>

<style>
  .step-picker {
    position: fixed;
    inset: auto;
    width: 280px;
    max-width: calc(100vw - 24px);
    max-height: min(480px, calc(100dvh - 24px));
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--dialog-shadow);
    overflow: hidden;
  }
  .step-picker:popover-open {
    display: flex;
    flex-direction: column;
  }
  .picker-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px 6px;
    font-size: 14px;
  }
  .picker-heading button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--dim);
  }
  .picker-search {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
    margin: 0 12px 8px;
    color: var(--dim);
  }
  .picker-search input {
    min-width: 0;
    width: 100%;
    min-height: 36px;
    font-size: 13px;
  }
  .insert-hint {
    margin: 0 12px 8px;
    color: var(--dim);
    font-size: 12px;
  }
  .connection-choice {
    margin: 0 12px 8px;
  }
  .picker-list {
    overflow-y: auto;
    scrollbar-gutter: stable;
    overscroll-behavior: contain;
    min-height: 0;
    padding: 0 6px 6px;
  }
  h3 {
    padding: 8px;
    color: var(--dim);
    font-size: 12px;
    font-weight: 500;
  }
  .step-choice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 9px 8px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text);
    text-align: left;
  }
  .step-choice:hover:not(:disabled),
  .step-choice:focus-visible {
    background: var(--accentsoft);
  }
  .step-choice:disabled {
    opacity: 0.5;
  }
  .step-choice strong {
    display: block;
    font-size: 13px;
    font-weight: 600;
  }
  .step-choice small {
    display: block;
    margin-top: 3px;
    color: var(--dim);
    font-size: 11px;
    line-height: 1.6;
  }
  .empty {
    padding: 18px 8px;
    color: var(--dim);
    font-size: 13px;
  }
</style>
