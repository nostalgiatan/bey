# BEY GUI 模块

使用 Tauri 2.x 实现的图形用户界面模块。

## 状态

🚧 **开发中** - 基础框架已搭建，功能实现进行中

## 功能规划

- [ ] Tauri 应用程序初始化
- [ ] 前端页面框架（使用 React/Vue/Svelte）
- [ ] 设备列表实时更新
- [ ] 文件传输界面
- [ ] 消息通知
- [ ] 系统托盘集成
- [ ] 设置界面
- [ ] 多语言支持

## 技术栈

- **后端**: Rust + Tauri 2.x
- **前端**: (待定) React/Vue/Svelte + TypeScript
- **样式**: Tailwind CSS
- **状态管理**: (待定)

## 开发指南

### 前置要求

1. Node.js 18+ 和 npm/pnpm
2. Rust 工具链
3. Tauri CLI: `cargo install tauri-cli`

### 初始化 Tauri 项目

```bash
cd src/crates/bey-gui
cargo tauri init
```

### 开发模式

```bash
cargo tauri dev
```

### 构建

```bash
cargo tauri build
```

## 架构设计

```
bey-gui/
├── src/                    # Rust 后端代码
│   └── lib.rs             # GUI 主模块
├── src-tauri/             # Tauri 配置和图标
│   ├── tauri.conf.json   # Tauri 配置文件
│   └── icons/            # 应用图标
└── ui/                    # 前端代码（待创建）
    ├── src/
    ├── public/
    └── package.json
```

## 与 BEY 核心集成

GUI 模块通过 `BeyFuncManager` 与核心功能模块通信：

```rust
use bey_gui::GuiApp;
use bey_func::BeyFuncManager;

let manager = BeyFuncManager::new(device_id, storage_path).await?;
let gui = GuiApp::new(manager);
gui.run().await?;
```

## Tauri 命令示例

```rust
#[tauri::command]
async fn get_devices(state: tauri::State<'_, AppState>) -> Result<Vec<DeviceInfo>, String> {
    let devices = state.manager.engine().list_discovered_devices().await;
    Ok(devices)
}

#[tauri::command]
async fn send_message(
    state: tauri::State<'_, AppState>,
    device_id: String,
    message: String,
) -> Result<(), String> {
    state.manager.send_message(&device_id, message).await
        .map_err(|e| e.to_string())
}
```

## 界面设计

### 主窗口布局

```
┌─────────────────────────────────────────────────┐
│  BEY - 局域网协作中心                  [_ □ ×]  │
├─────────────────────────────────────────────────┤
│  📱 设备  │  📁 文件  │  💬 消息  │  ⚙️ 设置   │
├──────────┼──────────────────────────────────────┤
│          │                                       │
│ 设备列表 │         主内容区域                    │
│          │                                       │
│ • 本地   │  - 设备详情                          │
│ • 设备A  │  - 文件浏览器                        │
│ • 设备B  │  - 传输进度                          │
│          │  - 消息历史                          │
│          │                                       │
└──────────┴───────────────────────────────────────┘
```

## 待办事项

1. ✅ 创建 GUI 模块目录结构
2. ✅ 添加基础 Cargo.toml 和 lib.rs
3. ⬜ 初始化 Tauri 项目配置
4. ⬜ 创建前端项目（选择框架）
5. ⬜ 实现设备列表界面
6. ⬜ 实现文件传输界面
7. ⬜ 实现消息界面
8. ⬜ 实现系统托盘
9. ⬜ 添加集成测试

## 相关文档

- [Tauri 官方文档](https://tauri.app/)
- [Tauri 2.0 迁移指南](https://tauri.app/v2/guides/)
- [BEY 项目架构文档](../../README.md)
