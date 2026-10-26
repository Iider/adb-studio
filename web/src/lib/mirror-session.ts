const key = "adb-studio-mirror";
const restoredPages = new WeakSet<object>();

export function rememberMirror(id: string, storage?: Storage) {
  try {
    (storage ?? sessionStorage).setItem(key, id);
  } catch {
    // 浏览器禁用会话存储时，仍允许手动投屏。
  }
}

export function forgetMirror(id: string, storage?: Storage) {
  try {
    const session = storage ?? sessionStorage;
    if (session.getItem(key) === id) session.removeItem(key);
  } catch {
    // 当前连接仍按正常流程释放。
  }
}

export function restoreMirror(
  id: string,
  navigation: string,
  storage?: Storage,
  page?: object,
): boolean {
  try {
    if (page) {
      if (restoredPages.has(page)) return false;
      restoredPages.add(page);
    }
    return (
      navigation === "reload" && (storage ?? sessionStorage).getItem(key) === id
    );
  } catch {
    return false;
  }
}
