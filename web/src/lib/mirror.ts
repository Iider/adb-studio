import { ScrcpyOptions3_1 } from "@yume-chan/scrcpy";
import { WebCodecsVideoDecoder } from "@yume-chan/scrcpy-decoder-webcodecs";
import { ReadableStream } from "@yume-chan/stream-extra";

export function startMirror(
  id: string,
  canvas: HTMLCanvasElement,
  onFrame: (width: number, height: number) => void,
  onError: (message: string) => void,
) {
  const socket = new WebSocket(
    `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/devices/${encodeURIComponent(id)}/mirror`,
  );
  socket.binaryType = "arraybuffer";
  let stopped = false;
  let decoder: WebCodecsVideoDecoder | undefined;
  const timeout = setTimeout(
    () => fail("投屏启动超时，请解锁手机并重试"),
    20_000,
  );
  let controller: ReadableStreamDefaultController<Uint8Array>;
  const abort = new AbortController();
  const context = canvas.getContext("2d", { alpha: false });
  const stream = new ReadableStream<Uint8Array>(
    {
      start(value) {
        controller = value;
      },
      cancel() {
        stop();
      },
    },
    { highWaterMark: 8 * 1024 * 1024, size: (chunk) => chunk.byteLength },
  );

  function stop() {
    if (stopped) return;
    stopped = true;
    clearTimeout(timeout);
    abort.abort();
    socket.close();
    controller.error(new Error("投屏已停止"));
    decoder?.dispose();
    context?.clearRect(0, 0, canvas.width, canvas.height);
  }
  function fail(message: string) {
    if (stopped) return;
    stop();
    onError(message);
  }
  socket.onmessage = (event) => {
    if (stopped) return;
    if (typeof event.data === "string") {
      try {
        fail(JSON.parse(event.data).error || "投屏已结束");
      } catch {
        fail("投屏返回异常，请重新开启");
      }
      return;
    }
    if ((controller.desiredSize ?? 0) <= 0) {
      fail("画面处理过慢，请重新开启投屏");
      return;
    }
    controller.enqueue(new Uint8Array(event.data));
  };
  socket.onerror = () => fail("投屏连接失败，请检查工作台连接后重试");
  socket.onclose = () => fail("投屏连接已中断，请检查手机后重新开启");

  void (async () => {
    try {
      const options = new ScrcpyOptions3_1({
        sendDeviceMeta: false,
        sendDummyByte: false,
        audio: false,
        control: false,
      });
      const video = await options.parseVideoStreamMetadata(stream);
      if (stopped) return;
      if (!context) throw new Error("浏览器无法显示投屏画面");
      decoder = new WebCodecsVideoDecoder({
        codec: video.metadata.codec,
        renderer: {
          setSize(width, height) {
            canvas.width = width;
            canvas.height = height;
          },
          draw(frame) {
            if (stopped) return;
            context.drawImage(frame, 0, 0, canvas.width, canvas.height);
            clearTimeout(timeout);
            onFrame(canvas.width, canvas.height);
          },
        },
      });
      await video.stream
        .pipeThrough(options.createMediaStreamTransformer())
        .pipeTo(decoder.writable, { signal: abort.signal });
      fail("手机投屏已结束，请重新开启");
    } catch (error) {
      fail(
        error instanceof Error
          ? `无法显示画面：${error.message}`
          : "无法显示投屏画面",
      );
    }
  })();
  return stop;
}
