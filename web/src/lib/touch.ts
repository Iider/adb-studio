export interface ScreenPoint {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface ScreenRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

// object-fit: contain 的黑边不属于手机画面；拖动出界时收敛到屏幕边缘。
export function screenPoint(
  rect: ScreenRect,
  width: number,
  height: number,
  clientX: number,
  clientY: number,
  clamp = false,
): ScreenPoint | undefined {
  if (rect.width <= 0 || rect.height <= 0 || width <= 0 || height <= 0) return;
  const scale = Math.min(rect.width / width, rect.height / height);
  const left = rect.left + (rect.width - width * scale) / 2;
  const top = rect.top + (rect.height - height * scale) / 2;
  const x = (clientX - left) / scale;
  const y = (clientY - top) / scale;
  if (!clamp && (x < 0 || y < 0 || x >= width || y >= height)) return;
  return {
    x: Math.max(0, Math.min(width - 1, Math.floor(x))),
    y: Math.max(0, Math.min(height - 1, Math.floor(y))),
    width,
    height,
  };
}
