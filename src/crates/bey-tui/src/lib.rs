//! # BEY TUI 模块
//!
//! 提供基于终端的用户界面，使用 ratatui 实现。
//!
//! ## 功能
//!
//! - 设备列表视图
//! - 实时日志查看器
//! - 状态监控面板
//! - 交互式命令输入
//! - 消息发送功能（私信、群聊、广播）
//! - 剪切板同步功能
//! - 文件传输功能
//!
//! ## 使用示例
//!
//! ```no_run
//! use bey_tui::TuiApp;
//! use bey_func::BeyFuncManager;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let manager = BeyFuncManager::new("my_device", "./storage").await?;
//!     let mut tui = TuiApp::new(Arc::new(manager));
//!     tui.run().await?;
//!     Ok(())
//! }
//! ```

use error::ErrorInfo;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};
use std::sync::Arc;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use bey_func::BeyFuncManager;

pub type TuiResult<T> = Result<T, ErrorInfo>;

/// TUI 应用状态
#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    /// 正常浏览模式
    Normal,
    /// 命令输入模式
    Command,
    /// 帮助模式
    Help,
    /// 操作菜单模式
    OperationMenu,
    /// 输入表单模式
    InputForm(OperationType),
}

/// 操作类型
#[derive(Debug, Clone, PartialEq)]
pub enum OperationType {
    /// 发送私信
    SendPrivateMessage,
    /// 发送群聊消息
    SendGroupMessage,
    /// 广播消息
    BroadcastMessage,
    /// 添加剪切板
    AddClipboard,
    /// 同步剪切板到对等设备
    SyncClipboardToPeer,
    /// 同步剪切板到群组
    SyncClipboardToGroup,
    /// 上传文件到云存储
    UploadToCloud,
    /// 从云存储下载文件
    DownloadFromCloud,
    /// 发送文件到对等设备
    SendFileToPeer,
}

/// TUI 日志条目
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Instant,
    pub level: LogLevel,
    pub message: String,
}

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogLevel {
    fn color(&self) -> Color {
        match self {
            LogLevel::Info => Color::Green,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
            LogLevel::Debug => Color::Blue,
        }
    }

    fn prefix(&self) -> &'static str {
        match self {
            LogLevel::Info => "[INFO] ",
            LogLevel::Warn => "[WARN] ",
            LogLevel::Error => "[ERROR]",
            LogLevel::Debug => "[DEBUG]",
        }
    }
}

/// TUI 应用程序
pub struct TuiApp {
    /// 功能管理器
    manager: Arc<BeyFuncManager>,
    /// 当前模式
    mode: AppMode,
    /// 命令输入缓冲
    command_input: String,
    /// 日志条目
    logs: Vec<LogEntry>,
    /// 最大日志条目数
    max_logs: usize,
    /// 是否需要退出
    should_quit: bool,
    /// 选中的设备索引
    selected_device: usize,
    /// 选中的操作索引
    selected_operation: usize,
    /// 表单输入字段
    form_fields: Vec<(String, String)>, // (field_name, field_value)
    /// 当前聚焦的表单字段
    focused_field: usize,
}

impl TuiApp {
    /// 创建新的 TUI 应用程序
    ///
    /// # 参数
    ///
    /// * `manager` - BEY 功能管理器
    ///
    /// # 返回
    ///
    /// 返回新创建的 TUI 应用程序实例
    pub fn new(manager: Arc<BeyFuncManager>) -> Self {
        Self {
            manager,
            mode: AppMode::Normal,
            command_input: String::new(),
            logs: Vec::new(),
            max_logs: 1000,
            should_quit: false,
            selected_device: 0,
            selected_operation: 0,
            form_fields: Vec::new(),
            focused_field: 0,
        }
    }

    /// 添加日志条目
    pub fn add_log(&mut self, level: LogLevel, message: String) {
        self.logs.push(LogEntry {
            timestamp: Instant::now(),
            level,
            message,
        });

        // 限制日志数量
        if self.logs.len() > self.max_logs {
            self.logs.remove(0);
        }
    }

    /// 运行 TUI 应用程序
    ///
    /// # 错误
    ///
    /// 如果终端初始化失败或运行过程中发生错误，返回错误信息
    pub async fn run(&mut self) -> TuiResult<()> {
        // 设置终端
        enable_raw_mode().map_err(|e| {
            ErrorInfo::new(9000, "TUI错误".to_string())
                .with_context(format!("启用原始模式失败: {}", e))
        })?;

        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| {
            ErrorInfo::new(9000, "TUI错误".to_string())
                .with_context(format!("进入备用屏幕失败: {}", e))
        })?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(|e| {
            ErrorInfo::new(9000, "TUI错误".to_string())
                .with_context(format!("创建终端失败: {}", e))
        })?;

        self.add_log(LogLevel::Info, "BEY TUI 启动".to_string());
        self.add_log(
            LogLevel::Info,
            format!("设备ID: {}", self.manager.device_id()),
        );

        // 运行主循环
        let result = self.run_loop(&mut terminal).await;

        // 恢复终端
        disable_raw_mode().map_err(|e| {
            ErrorInfo::new(9000, "TUI错误".to_string())
                .with_context(format!("禁用原始模式失败: {}", e))
        })?;

        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(|e| {
            ErrorInfo::new(9000, "TUI错误".to_string())
                .with_context(format!("离开备用屏幕失败: {}", e))
        })?;

        terminal.show_cursor().map_err(|e| {
            ErrorInfo::new(9000, "TUI错误".to_string())
                .with_context(format!("显示光标失败: {}", e))
        })?;

        result
    }

    /// 主事件循环
    async fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> TuiResult<()> {
        let tick_rate = Duration::from_millis(250);
        let mut last_tick = Instant::now();

        loop {
            terminal
                .draw(|f| self.ui(f))
                .map_err(|e| {
                    ErrorInfo::new(9000, "TUI错误".to_string())
                        .with_context(format!("绘制界面失败: {}", e))
                })?;

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout).map_err(|e| {
                ErrorInfo::new(9000, "TUI错误".to_string())
                    .with_context(format!("轮询事件失败: {}", e))
            })? {
                if let Event::Key(key) = event::read().map_err(|e| {
                    ErrorInfo::new(9000, "TUI错误".to_string())
                        .with_context(format!("读取事件失败: {}", e))
                })? {
                    self.handle_key_event(key).await;
                }
            }

            if last_tick.elapsed() >= tick_rate {
                self.on_tick().await;
                last_tick = Instant::now();
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// 处理按键事件
    async fn handle_key_event(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Normal => {
                match key.code {
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.should_quit = true
                    }
                    KeyCode::Char(':') => {
                        self.mode = AppMode::Command;
                        self.command_input.clear();
                    }
                    KeyCode::Char('?') => self.mode = AppMode::Help,
                    KeyCode::Char('o') | KeyCode::Char('O') => {
                        self.mode = AppMode::OperationMenu;
                        self.selected_operation = 0;
                    }
                    KeyCode::Up => {
                        if self.selected_device > 0 {
                            self.selected_device -= 1;
                        }
                    }
                    KeyCode::Down => {
                        self.selected_device += 1;
                    }
                    _ => {}
                }
            }
            AppMode::OperationMenu => {
                match key.code {
                    KeyCode::Up => {
                        if self.selected_operation > 0 {
                            self.selected_operation -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if self.selected_operation < 8 {
                            self.selected_operation += 1;
                        }
                    }
                    KeyCode::Enter => {
                        self.start_operation(self.selected_operation).await;
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = AppMode::Normal;
                    }
                    _ => {}
                }
            }
            AppMode::InputForm(ref op_type) => {
                match key.code {
                    KeyCode::Enter => {
                        self.execute_operation(op_type.clone()).await;
                        self.mode = AppMode::Normal;
                    }
                    KeyCode::Esc => {
                        self.mode = AppMode::OperationMenu;
                    }
                    KeyCode::Tab => {
                        if self.focused_field < self.form_fields.len() - 1 {
                            self.focused_field += 1;
                        }
                    }
                    KeyCode::BackTab => {
                        if self.focused_field > 0 {
                            self.focused_field -= 1;
                        }
                    }
                    KeyCode::Char(c) => {
                        if !self.form_fields.is_empty() {
                            self.form_fields[self.focused_field].1.push(c);
                        }
                    }
                    KeyCode::Backspace => {
                        if !self.form_fields.is_empty() {
                            self.form_fields[self.focused_field].1.pop();
                        }
                    }
                    _ => {}
                }
            }
            AppMode::Command => {
                match key.code {
                    KeyCode::Enter => {
                        let cmd = self.command_input.clone();
                        self.execute_command(&cmd).await;
                        self.command_input.clear();
                        self.mode = AppMode::Normal;
                    }
                    KeyCode::Esc => {
                        self.command_input.clear();
                        self.mode = AppMode::Normal;
                    }
                    KeyCode::Char(c) => {
                        self.command_input.push(c);
                    }
                    KeyCode::Backspace => {
                        self.command_input.pop();
                    }
                    _ => {}
                }
            }
            AppMode::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                    self.mode = AppMode::Normal;
                }
            }
        }
    }

    /// 执行命令
    async fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "quit" | "q" => {
                self.should_quit = true;
            }
            "clear" => {
                self.logs.clear();
                self.add_log(LogLevel::Info, "日志已清空".to_string());
            }
            "help" => {
                self.mode = AppMode::Help;
            }
            "devices" => {
                let devices = self.manager.engine().list_discovered_devices().await;
                self.add_log(
                    LogLevel::Info,
                    format!("发现 {} 个设备", devices.len()),
                );
            }
            _ => {
                self.add_log(
                    LogLevel::Warn,
                    format!("未知命令: {}", parts[0]),
                );
            }
        }
    }

    /// 启动操作并准备输入表单
    async fn start_operation(&mut self, operation_index: usize) {
        self.form_fields.clear();
        self.focused_field = 0;

        let op_type = match operation_index {
            0 => {
                self.form_fields.push(("设备ID".to_string(), String::new()));
                self.form_fields.push(("消息内容".to_string(), String::new()));
                OperationType::SendPrivateMessage
            }
            1 => {
                self.form_fields.push(("群组ID".to_string(), String::new()));
                self.form_fields.push(("消息内容".to_string(), String::new()));
                OperationType::SendGroupMessage
            }
            2 => {
                self.form_fields.push(("消息内容".to_string(), String::new()));
                OperationType::BroadcastMessage
            }
            3 => {
                self.form_fields.push(("内容类型".to_string(), String::new()));
                self.form_fields.push(("内容".to_string(), String::new()));
                OperationType::AddClipboard
            }
            4 => {
                self.form_fields.push(("设备ID".to_string(), String::new()));
                OperationType::SyncClipboardToPeer
            }
            5 => {
                self.form_fields.push(("群组ID".to_string(), String::new()));
                OperationType::SyncClipboardToGroup
            }
            6 => {
                self.form_fields.push(("文件名".to_string(), String::new()));
                self.form_fields.push(("文件内容".to_string(), String::new()));
                OperationType::UploadToCloud
            }
            7 => {
                self.form_fields.push(("文件哈希".to_string(), String::new()));
                OperationType::DownloadFromCloud
            }
            8 => {
                self.form_fields.push(("设备ID".to_string(), String::new()));
                self.form_fields.push(("文件名".to_string(), String::new()));
                self.form_fields.push(("文件内容".to_string(), String::new()));
                OperationType::SendFileToPeer
            }
            _ => return,
        };

        self.mode = AppMode::InputForm(op_type);
    }

    /// 执行操作
    async fn execute_operation(&mut self, op_type: OperationType) {
        match op_type {
            OperationType::SendPrivateMessage => {
                if self.form_fields.len() >= 2 {
                    let peer_id = &self.form_fields[0].1;
                    let content = self.form_fields[1].1.as_bytes();
                    match self.manager.send_private_message(peer_id, content).await {
                        Ok(msg_id) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("私信已发送到 {}, ID: {}", peer_id, msg_id),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("发送私信失败: {}", e),
                            );
                        }
                    }
                }
            }
            OperationType::SendGroupMessage => {
                if self.form_fields.len() >= 2 {
                    let group_id = &self.form_fields[0].1;
                    let content = self.form_fields[1].1.as_bytes();
                    match self.manager.send_group_message(group_id, content).await {
                        Ok(msg_id) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("群聊消息已发送到 {}, ID: {}", group_id, msg_id),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("发送群聊消息失败: {}", e),
                            );
                        }
                    }
                }
            }
            OperationType::BroadcastMessage => {
                if !self.form_fields.is_empty() {
                    let content = self.form_fields[0].1.as_bytes();
                    match self.manager.broadcast_message(content).await {
                        Ok(count) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("广播消息已发送到 {} 个设备", count),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("广播消息失败: {}", e),
                            );
                        }
                    }
                }
            }
            OperationType::AddClipboard => {
                if self.form_fields.len() >= 2 {
                    let content_type = &self.form_fields[0].1;
                    let content = self.form_fields[1].1.as_bytes();
                    match self.manager.add_clipboard(content_type, content).await {
                        Ok(entry_id) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("剪切板内容已添加, ID: {}", entry_id),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("添加剪切板失败: {}", e),
                            );
                        }
                    }
                }
            }
            OperationType::SyncClipboardToPeer => {
                if !self.form_fields.is_empty() {
                    let peer_id = &self.form_fields[0].1;
                    match self.manager.sync_clipboard_to_peer(peer_id).await {
                        Ok(_) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("剪切板已同步到 {}", peer_id),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("同步剪切板失败: {}", e),
                            );
                        }
                    }
                }
            }
            OperationType::SyncClipboardToGroup => {
                if !self.form_fields.is_empty() {
                    let group_id = &self.form_fields[0].1;
                    match self.manager.sync_clipboard_to_group(group_id).await {
                        Ok(_) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("剪切板已同步到群组 {}", group_id),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("同步剪切板到群组失败: {}", e),
                            );
                        }
                    }
                }
            }
            OperationType::UploadToCloud => {
                if self.form_fields.len() >= 2 {
                    let filename = &self.form_fields[0].1;
                    let data = self.form_fields[1].1.as_bytes();
                    match self.manager.upload_to_cloud(filename, data).await {
                        Ok(file_hash) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("文件已上传到云存储, 哈希: {}", file_hash),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("上传文件失败: {}", e),
                            );
                        }
                    }
                }
            }
            OperationType::DownloadFromCloud => {
                if !self.form_fields.is_empty() {
                    let file_hash = &self.form_fields[0].1;
                    match self.manager.download_from_cloud(file_hash).await {
                        Ok(data) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("文件已从云存储下载, 大小: {} 字节", data.len()),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("下载文件失败: {}", e),
                            );
                        }
                    }
                }
            }
            OperationType::SendFileToPeer => {
                if self.form_fields.len() >= 3 {
                    let peer_id = &self.form_fields[0].1;
                    let filename = &self.form_fields[1].1;
                    let data = self.form_fields[2].1.as_bytes();
                    match self.manager.send_file_to_peer(peer_id, filename, data).await {
                        Ok(_) => {
                            self.add_log(
                                LogLevel::Info,
                                format!("文件 {} 已发送到 {}", filename, peer_id),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                LogLevel::Error,
                                format!("发送文件失败: {}", e),
                            );
                        }
                    }
                }
            }
        }
    }

    /// 定时更新
    async fn on_tick(&mut self) {
        // 这里可以添加定时任务，比如更新设备列表等
    }

    /// 绘制UI
    fn ui(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),      // 标题
                Constraint::Min(10),         // 主内容
                Constraint::Length(3),       // 状态栏
            ])
            .split(f.area());

        // 标题栏
        self.render_title(f, chunks[0]);

        match self.mode {
            AppMode::Normal => {
                // 主内容区域分为左右两部分
                let main_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                    .split(chunks[1]);

                self.render_device_list(f, main_chunks[0]);
                self.render_logs(f, main_chunks[1]);
            }
            AppMode::Help => {
                self.render_help(f, chunks[1]);
            }
            AppMode::OperationMenu => {
                self.render_operation_menu(f, chunks[1]);
            }
            AppMode::InputForm(_) => {
                self.render_input_form(f, chunks[1]);
            }
            AppMode::Command => {
                // 命令模式下也显示主内容
                let main_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                    .split(chunks[1]);

                self.render_device_list(f, main_chunks[0]);
                self.render_logs(f, main_chunks[1]);
            }
        }

        // 状态栏
        self.render_status(f, chunks[2]);
    }

    /// 渲染标题栏
    fn render_title(&self, f: &mut Frame, area: Rect) {
        let title = Paragraph::new("BEY - 分布式文件传输系统 (TUI)")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, area);
    }

    /// 渲染设备列表
    fn render_device_list(&self, f: &mut Frame, area: Rect) {
        let devices: Vec<ListItem> = vec![
            ListItem::new(format!("本地设备: {}", self.manager.device_id())),
            ListItem::new(""),
            ListItem::new("发现的设备:"),
            ListItem::new("  (按方向键选择)"),
        ];

        let list = List::new(devices)
            .block(
                Block::default()
                    .title("设备列表")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol(">> ");

        f.render_widget(list, area);
    }

    /// 渲染日志
    fn render_logs(&self, f: &mut Frame, area: Rect) {
        let logs: Vec<Line> = self
            .logs
            .iter()
            .rev()
            .take(area.height as usize - 2)
            .map(|entry| {
                Line::from(vec![
                    Span::styled(
                        entry.level.prefix(),
                        Style::default().fg(entry.level.color()),
                    ),
                    Span::raw(" "),
                    Span::raw(&entry.message),
                ])
            })
            .collect();

        let logs_widget = Paragraph::new(logs)
            .block(
                Block::default()
                    .title("日志")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .wrap(Wrap { trim: true });

        f.render_widget(logs_widget, area);
    }

    /// 渲染帮助信息
    fn render_help(&self, f: &mut Frame, area: Rect) {
        let help_text = vec![
            Line::from(""),
            Line::from(Span::styled(
                "快捷键",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  q         - 退出程序"),
            Line::from("  Ctrl+C    - 退出程序"),
            Line::from("  o         - 打开操作菜单"),
            Line::from("  :         - 进入命令模式"),
            Line::from("  ?         - 显示/隐藏帮助"),
            Line::from("  ↑/↓       - 选择设备"),
            Line::from(""),
            Line::from(Span::styled(
                "命令",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  :quit     - 退出程序"),
            Line::from("  :clear    - 清空日志"),
            Line::from("  :devices  - 列出设备"),
            Line::from("  :help     - 显示帮助"),
            Line::from(""),
            Line::from(Span::styled(
                "操作菜单 (按 'o' 打开)",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  1. 发送私信"),
            Line::from("  2. 发送群聊消息"),
            Line::from("  3. 广播消息"),
            Line::from("  4. 添加剪切板"),
            Line::from("  5. 同步剪切板到对等设备"),
            Line::from("  6. 同步剪切板到群组"),
            Line::from("  7. 上传文件到云存储"),
            Line::from("  8. 从云存储下载文件"),
            Line::from("  9. 发送文件到对等设备"),
            Line::from(""),
            Line::from("按 '?' 或 ESC 返回"),
        ];

        let help = Paragraph::new(help_text)
            .block(
                Block::default()
                    .title("帮助")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(help, area);
    }

    /// 渲染状态栏
    fn render_status(&self, f: &mut Frame, area: Rect) {
        let mode_text = match self.mode {
            AppMode::Normal => "正常模式 | 按 'o' 打开操作菜单 | 按 ':' 输入命令 | 按 '?' 查看帮助 | 按 'q' 退出",
            AppMode::Command => {
                return self.render_command_input(f, area);
            }
            AppMode::Help => "帮助模式 | 按 '?' 或 ESC 返回",
            AppMode::OperationMenu => "操作菜单 | ↑↓ 选择 | Enter 确认 | ESC 返回",
            AppMode::InputForm(_) => "输入表单 | Tab 切换字段 | Enter 提交 | ESC 返回菜单",
        };

        let status = Paragraph::new(mode_text)
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL));

        f.render_widget(status, area);
    }

    /// 渲染命令输入
    fn render_command_input(&self, f: &mut Frame, area: Rect) {
        let input = Paragraph::new(format!(":{}", self.command_input))
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("命令 (Enter 执行, ESC 取消)"),
            );

        f.render_widget(input, area);
    }

    /// 渲染操作菜单
    fn render_operation_menu(&self, f: &mut Frame, area: Rect) {
        let operations = vec![
            "1. 发送私信",
            "2. 发送群聊消息",
            "3. 广播消息",
            "4. 添加剪切板",
            "5. 同步剪切板到对等设备",
            "6. 同步剪切板到群组",
            "7. 上传文件到云存储",
            "8. 从云存储下载文件",
            "9. 发送文件到对等设备",
        ];

        let items: Vec<ListItem> = operations
            .iter()
            .enumerate()
            .map(|(i, op)| {
                let style = if i == self.selected_operation {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(*op).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title("操作菜单 - 选择一个操作")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        f.render_widget(list, area);
    }

    /// 渲染输入表单
    fn render_input_form(&self, f: &mut Frame, area: Rect) {
        let mut lines = Vec::new();
        lines.push(Line::from(""));

        for (i, (field_name, field_value)) in self.form_fields.iter().enumerate() {
            let is_focused = i == self.focused_field;
            let style = if is_focused {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_focused { "▶ " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{}: ", field_name), style),
            ]));
            
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(field_value.clone(), style),
                Span::styled(if is_focused { "█" } else { "" }, style),
            ]));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Tab - 切换字段 | Enter - 提交 | ESC - 返回",
            Style::default().fg(Color::Gray),
        )));

        let form = Paragraph::new(lines)
            .block(
                Block::default()
                    .title("输入表单")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green)),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(form, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_color() {
        assert_eq!(LogLevel::Info.color(), Color::Green);
        assert_eq!(LogLevel::Warn.color(), Color::Yellow);
        assert_eq!(LogLevel::Error.color(), Color::Red);
        assert_eq!(LogLevel::Debug.color(), Color::Blue);
    }

    #[test]
    fn test_log_level_prefix() {
        assert_eq!(LogLevel::Info.prefix(), "[INFO] ");
        assert_eq!(LogLevel::Warn.prefix(), "[WARN] ");
        assert_eq!(LogLevel::Error.prefix(), "[ERROR]");
        assert_eq!(LogLevel::Debug.prefix(), "[DEBUG]");
    }

    #[test]
    fn test_app_mode() {
        assert_eq!(AppMode::Normal, AppMode::Normal);
        assert_ne!(AppMode::Normal, AppMode::Command);
    }
}
