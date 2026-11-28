# 从源码构建

**环境要求：** Node.js 24+、pnpm、较新的 Rust 工具链（1.90+），以及你所在系统的 [Tauri 前置依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
git clone https://github.com/jazzenchen/agent-hud-open.git
cd agent-hud-open
pnpm install
pnpm run check
pnpm run build:macos -- --bundle
```

在 Windows 或 Linux 上改用 `build:windows` 或 `build:linux`；每条命令只为它所在的系统构建。Tauri 把应用写在 `target/release/bundle` 下。想在没有安装任何 agent 时试用，用 `--demo` 启动，它会用示例数据打开应用。

**修改过的 Tao。** HUD 在 macOS 上必须是一个不抢焦点的面板，不会把你正在用的应用切到后台；上游 Tao 目前还创建不了这种窗口（[tao#414](https://github.com/tauri-apps/tao/issues/414)）。在上游支持之前，工作区从[一个公开的分支](https://github.com/jazzenchen/tao/tree/agent-hud/nonactivating-panel)取 `tao`：在 0.37.1 之上只多一个小提交，在 `Cargo.toml` 里按提交固定，首次构建时由 Cargo 从 GitHub 拉取。改了什么、以后怎样换到新的 Tao 版本，见 [Tao 补丁](tao-panel-patch.md)。

**预览版。** 这个构建正逐项对照原生 Swift 版 [agent-hud-native-open](https://github.com/jazzenchen/agent-hud-native-open)（原 agent-hud-open）的行为，见[对齐清单](parity/README.md)。它暂不安装客户端回调，因此批准、提问与大部分回复仍留在各 agent 自己的提示里。

<details>
<summary><strong>开发</strong></summary>

| 命令 | 用途 |
| --- | --- |
| `pnpm run check` | 构建界面与 MCP 服务，检查平台边界与对齐清单，对 Rust 做类型检查 |
| `cargo test --workspace` | 运行 Rust 测试 |
| `pnpm run dev` | 在浏览器中以实时数据运行界面，地址 `http://127.0.0.1:1420`（先运行 `pnpm run engine:build`） |
| `pnpm run build:macos` / `build:windows` / `build:linux` | 为当前系统构建应用 |
| `pnpm --silent run mcp` | 通过 stdio 运行 MCP 服务 |

工作区结构、MCP 客户端配置与其他命令见[开发文档](development.md)。代码放在哪里，按[贡献指南](../CONTRIBUTING.md)：核心负责计算，界面只负责显示，每个系统的代码留在各自的文件里。

</details>
