# 来源声明

## bili-sync

来源：https://github.com/amtoaer/bili-sync ，参考版本 `c900777`，MIT。

沿用 Rust / Tokio / Axum、SvelteKit 静态网页、SeaORM / SQLite 的应用组织方式与状态推送思路。

以下组件复制并使用：

- `web/src/lib/components/ui/button/button.svelte`
- `web/src/lib/components/ui/badge/badge.svelte`
- `web/src/lib/utils.ts` 的 `cn` 与 `WithElementRef`

这些组件沿用 bili-sync 的许可证：

> MIT License
>
> Copyright (c) 2024 ᴀᴍᴛᴏᴀᴇʀ
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

## VibeGamer

来源：https://github.com/karminski/VibeGamer ，参考版本 `201e103fe6e3`，MIT。

参考观察与行动分层、动作结果检查和验证通过后启用策略的设计。手机应用使用独立的 ADB 与图像观察实现。

## pi-ai

来源：https://github.com/earendil-works/pi/tree/main/packages/ai ，参考版本 `b9ab2a6238d2`，MIT。

模型配置参考其模型编号、接口类型、接口地址及鉴权分离方式。本应用由 Rust 直接请求配置的模型接口。
