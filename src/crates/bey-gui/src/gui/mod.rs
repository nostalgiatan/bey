//! # GUI 子模块
//!
//! 组织 GUI 相关的所有功能模块
//!
//! ## 模块结构
//!
//! - `commands` - Tauri 命令处理器实现
//! - `events` - 事件处理和分发
//! - `state` - GUI 应用状态管理

pub mod commands;
pub mod events;
pub mod state;

pub use commands::TauriCommandHandler;
pub use events::EventEmitter;
pub use state::GuiState;
