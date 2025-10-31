//! # BEY 网络有限状态机
//!
//! 实现基于有限状态机的网络连接管理，提供清晰的状态转换和事件处理。
//!
//! ## 状态机设计
//!
//! - **Idle**: 空闲状态，未建立连接
//! - **Connecting**: 正在建立连接
//! - **Connected**: 已建立连接
//! - **Authenticating**: 正在认证
//! - **Authenticated**: 已认证，可以传输数据
//! - **Disconnecting**: 正在断开连接
//! - **Error**: 错误状态

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, SystemTime};
use tracing::{debug, info, warn};

use crate::NetResult;

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConnectionState {
    /// 空闲状态
    Idle,
    /// 正在连接
    Connecting,
    /// 已连接
    Connected,
    /// 正在认证
    Authenticating,
    /// 已认证
    Authenticated,
    /// 正在传输数据
    Transferring,
    /// 正在断开连接
    Disconnecting,
    /// 已断开连接
    Disconnected,
    /// 错误状态
    Error,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionState::Idle => write!(f, "空闲"),
            ConnectionState::Connecting => write!(f, "正在连接"),
            ConnectionState::Connected => write!(f, "已连接"),
            ConnectionState::Authenticating => write!(f, "正在认证"),
            ConnectionState::Authenticated => write!(f, "已认证"),
            ConnectionState::Transferring => write!(f, "正在传输"),
            ConnectionState::Disconnecting => write!(f, "正在断开"),
            ConnectionState::Disconnected => write!(f, "已断开"),
            ConnectionState::Error => write!(f, "错误"),
        }
    }
}

/// 状态转换事件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateEvent {
    /// 开始连接
    Connect,
    /// 连接成功
    Connected,
    /// 开始认证
    Authenticate,
    /// 认证成功
    Authenticated,
    /// 认证失败
    AuthFailed,
    /// 开始传输数据
    StartTransfer,
    /// 传输完成
    TransferComplete,
    /// 断开连接
    Disconnect,
    /// 连接丢失
    ConnectionLost,
    /// 超时
    Timeout,
    /// 错误
    Error(String),
}

impl fmt::Display for StateEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateEvent::Connect => write!(f, "连接"),
            StateEvent::Connected => write!(f, "已连接"),
            StateEvent::Authenticate => write!(f, "认证"),
            StateEvent::Authenticated => write!(f, "已认证"),
            StateEvent::AuthFailed => write!(f, "认证失败"),
            StateEvent::StartTransfer => write!(f, "开始传输"),
            StateEvent::TransferComplete => write!(f, "传输完成"),
            StateEvent::Disconnect => write!(f, "断开连接"),
            StateEvent::ConnectionLost => write!(f, "连接丢失"),
            StateEvent::Timeout => write!(f, "超时"),
            StateEvent::Error(msg) => write!(f, "错误: {}", msg),
        }
    }
}

/// 状态转换
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// 源状态
    pub from: ConnectionState,
    /// 目标状态
    pub to: ConnectionState,
    /// 触发事件
    pub event: StateEvent,
    /// 转换时间
    pub timestamp: SystemTime,
}

/// 连接状态机
///
/// 管理连接的状态转换和事件处理
pub struct ConnectionStateMachine {
    /// 当前状态
    current_state: ConnectionState,
    /// 状态历史
    state_history: Vec<StateTransition>,
    /// 最大历史记录数
    max_history: usize,
    /// 连接建立时间
    connected_at: Option<SystemTime>,
    /// 认证完成时间
    authenticated_at: Option<SystemTime>,
}

impl ConnectionStateMachine {
    /// 创建新的状态机
    pub fn new() -> Self {
        Self {
            current_state: ConnectionState::Idle,
            state_history: Vec::new(),
            max_history: 100,
            connected_at: None,
            authenticated_at: None,
        }
    }

    /// 获取当前状态
    pub fn current_state(&self) -> ConnectionState {
        self.current_state
    }

    /// 检查是否可以发送数据
    pub fn can_transfer(&self) -> bool {
        matches!(
            self.current_state,
            ConnectionState::Authenticated | ConnectionState::Transferring
        )
    }

    /// 处理状态转换事件
    ///
    /// # 参数
    ///
    /// * `event` - 状态转换事件
    ///
    /// # 返回值
    ///
    /// 返回新状态或错误
    pub fn handle_event(&mut self, event: StateEvent) -> NetResult<ConnectionState> {
        let old_state = self.current_state;
        let new_state = self.compute_next_state(old_state, &event)?;

        // 验证状态转换是否有效
        if !self.is_valid_transition(old_state, new_state, &event) {
            return Err(ErrorInfo::new(
                4101,
                format!("无效的状态转换: {} -> {} (事件: {})", old_state, new_state, event),
            )
            .with_category(ErrorCategory::Validation)
            .with_severity(ErrorSeverity::Warning));
        }

        // 执行状态转换
        self.current_state = new_state;

        // 记录状态转换
        let transition = StateTransition {
            from: old_state,
            to: new_state,
            event: event.clone(),
            timestamp: SystemTime::now(),
        };
        self.add_to_history(transition);

        // 更新时间戳
        match new_state {
            ConnectionState::Connected => {
                self.connected_at = Some(SystemTime::now());
            }
            ConnectionState::Authenticated => {
                self.authenticated_at = Some(SystemTime::now());
            }
            ConnectionState::Disconnected | ConnectionState::Error => {
                self.connected_at = None;
                self.authenticated_at = None;
            }
            _ => {}
        }

        info!("状态转换: {} -> {} (事件: {})", old_state, new_state, event);
        Ok(new_state)
    }

    /// 计算下一个状态
    fn compute_next_state(
        &self,
        current: ConnectionState,
        event: &StateEvent,
    ) -> NetResult<ConnectionState> {
        let next = match (current, event) {
            // 从空闲状态开始连接
            (ConnectionState::Idle, StateEvent::Connect) => ConnectionState::Connecting,

            // 连接成功
            (ConnectionState::Connecting, StateEvent::Connected) => ConnectionState::Connected,

            // 开始认证
            (ConnectionState::Connected, StateEvent::Authenticate) => ConnectionState::Authenticating,

            // 认证成功
            (ConnectionState::Authenticating, StateEvent::Authenticated) => ConnectionState::Authenticated,

            // 认证失败
            (ConnectionState::Authenticating, StateEvent::AuthFailed) => ConnectionState::Error,

            // 开始传输数据
            (ConnectionState::Authenticated, StateEvent::StartTransfer) => ConnectionState::Transferring,

            // 传输完成，回到已认证状态
            (ConnectionState::Transferring, StateEvent::TransferComplete) => ConnectionState::Authenticated,

            // 从任何状态断开连接
            (_, StateEvent::Disconnect) => ConnectionState::Disconnecting,
            (ConnectionState::Disconnecting, _) => ConnectionState::Disconnected,

            // 连接丢失
            (_, StateEvent::ConnectionLost) => ConnectionState::Disconnected,

            // 超时
            (ConnectionState::Connecting, StateEvent::Timeout) => ConnectionState::Error,
            (ConnectionState::Authenticating, StateEvent::Timeout) => ConnectionState::Error,

            // 错误事件
            (_, StateEvent::Error(_)) => ConnectionState::Error,

            // 从错误状态恢复
            (ConnectionState::Error, StateEvent::Connect) => ConnectionState::Connecting,
            (ConnectionState::Disconnected, StateEvent::Connect) => ConnectionState::Connecting,

            // 其他情况保持当前状态
            _ => {
                warn!("未处理的状态转换: {} + {}", current, event);
                current
            }
        };

        Ok(next)
    }

    /// 验证状态转换是否有效
    fn is_valid_transition(
        &self,
        from: ConnectionState,
        to: ConnectionState,
        _event: &StateEvent,
    ) -> bool {
        // 相同状态的转换总是有效的（幂等性）
        if from == to {
            return true;
        }

        // 定义有效的状态转换
        match from {
            ConnectionState::Idle => matches!(to, ConnectionState::Connecting),
            ConnectionState::Connecting => matches!(
                to,
                ConnectionState::Connected | ConnectionState::Error | ConnectionState::Disconnected
            ),
            ConnectionState::Connected => matches!(
                to,
                ConnectionState::Authenticating | ConnectionState::Disconnecting | ConnectionState::Disconnected
            ),
            ConnectionState::Authenticating => matches!(
                to,
                ConnectionState::Authenticated | ConnectionState::Error | ConnectionState::Disconnected
            ),
            ConnectionState::Authenticated => matches!(
                to,
                ConnectionState::Transferring | ConnectionState::Disconnecting | ConnectionState::Disconnected
            ),
            ConnectionState::Transferring => matches!(
                to,
                ConnectionState::Authenticated | ConnectionState::Disconnecting | ConnectionState::Disconnected | ConnectionState::Error
            ),
            ConnectionState::Disconnecting => matches!(
                to,
                ConnectionState::Disconnected
            ),
            ConnectionState::Disconnected => matches!(
                to,
                ConnectionState::Connecting
            ),
            ConnectionState::Error => matches!(
                to,
                ConnectionState::Connecting | ConnectionState::Disconnected
            ),
        }
    }

    /// 添加到历史记录
    fn add_to_history(&mut self, transition: StateTransition) {
        self.state_history.push(transition);

        // 限制历史记录数量
        if self.state_history.len() > self.max_history {
            self.state_history.remove(0);
        }
    }

    /// 获取状态历史
    pub fn get_history(&self) -> &[StateTransition] {
        &self.state_history
    }

    /// 获取连接时长
    pub fn connection_duration(&self) -> Option<Duration> {
        self.connected_at.and_then(|connected_at| {
            SystemTime::now().duration_since(connected_at).ok()
        })
    }

    /// 重置状态机
    pub fn reset(&mut self) {
        debug!("重置状态机");
        self.current_state = ConnectionState::Idle;
        self.state_history.clear();
        self.connected_at = None;
        self.authenticated_at = None;
    }
}

impl Default for ConnectionStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_initial_state() {
        let sm = ConnectionStateMachine::new();
        assert_eq!(sm.current_state(), ConnectionState::Idle);
        assert!(!sm.can_transfer());
    }

    #[test]
    fn test_connection_flow() {
        let mut sm = ConnectionStateMachine::new();

        // 开始连接
        let state = sm.handle_event(StateEvent::Connect).unwrap();
        assert_eq!(state, ConnectionState::Connecting);

        // 连接成功
        let state = sm.handle_event(StateEvent::Connected).unwrap();
        assert_eq!(state, ConnectionState::Connected);

        // 开始认证
        let state = sm.handle_event(StateEvent::Authenticate).unwrap();
        assert_eq!(state, ConnectionState::Authenticating);

        // 认证成功
        let state = sm.handle_event(StateEvent::Authenticated).unwrap();
        assert_eq!(state, ConnectionState::Authenticated);
        assert!(sm.can_transfer());

        // 开始传输
        let state = sm.handle_event(StateEvent::StartTransfer).unwrap();
        assert_eq!(state, ConnectionState::Transferring);

        // 传输完成
        let state = sm.handle_event(StateEvent::TransferComplete).unwrap();
        assert_eq!(state, ConnectionState::Authenticated);
    }

    #[test]
    fn test_invalid_transition() {
        let mut sm = ConnectionStateMachine::new();

        // 尝试从空闲状态直接进入认证状态（无效）
        let result = sm.handle_event(StateEvent::Authenticated);
        assert!(result.is_ok()); // 保持在空闲状态
        assert_eq!(sm.current_state(), ConnectionState::Idle);
    }

    #[test]
    fn test_error_recovery() {
        let mut sm = ConnectionStateMachine::new();

        // 进入错误状态
        sm.handle_event(StateEvent::Error("测试错误".to_string())).unwrap();
        assert_eq!(sm.current_state(), ConnectionState::Error);

        // 从错误状态恢复
        let state = sm.handle_event(StateEvent::Connect).unwrap();
        assert_eq!(state, ConnectionState::Connecting);
    }

    #[test]
    fn test_state_history() {
        let mut sm = ConnectionStateMachine::new();

        sm.handle_event(StateEvent::Connect).unwrap();
        sm.handle_event(StateEvent::Connected).unwrap();

        let history = sm.get_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].from, ConnectionState::Idle);
        assert_eq!(history[0].to, ConnectionState::Connecting);
        assert_eq!(history[1].from, ConnectionState::Connecting);
        assert_eq!(history[1].to, ConnectionState::Connected);
    }

    #[test]
    fn test_connection_duration() {
        let mut sm = ConnectionStateMachine::new();

        // 未连接时应该返回None
        assert!(sm.connection_duration().is_none());

        // 连接后应该有时长
        sm.handle_event(StateEvent::Connect).unwrap();
        sm.handle_event(StateEvent::Connected).unwrap();
        assert!(sm.connection_duration().is_some());
    }
}
