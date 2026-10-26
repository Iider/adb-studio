export const scrollbarHideDelay = 1500;

export function installScrollbars(document: Document): () => void {
  const timers = new Map<HTMLElement, ReturnType<typeof setTimeout>>();

  function onScroll(event: Event) {
    const target =
      event.target === document ? document.scrollingElement : event.target;
    if (!(target instanceof HTMLElement)) return;

    clearTimeout(timers.get(target));
    target.setAttribute("data-scroll-active", "");
    timers.set(
      target,
      setTimeout(() => {
        target.removeAttribute("data-scroll-active");
        timers.delete(target);
      }, scrollbarHideDelay),
    );
  }

  document.addEventListener("scroll", onScroll, { capture: true, passive: true });
  return () => {
    document.removeEventListener("scroll", onScroll, true);
    for (const [target, timer] of timers) {
      clearTimeout(timer);
      target.removeAttribute("data-scroll-active");
    }
    timers.clear();
  };
}
