//! # BEY 权限控制模块
//!
//! 提供完整的基于角色（RBAC）的权限控制系统，支持角色管理、
//! 权限分配、访问控制和权限验证。包含高性能无依赖策略引擎。
//!
//! ## 核心特性
//!
//! - **RBAC 权限模型**: 基于角色的访问控制
//! - **策略引擎**: 高性能无外部依赖的策略评估引擎
//! - **细粒度权限**: 支持资源级别的精确权限控制
//! - **动态权限**: 运行时权限分配和撤销
//! - **规则引擎**: 基于条件的动态权限评估
//! - **权限继承**: 支持角色层级和权限继承
//! - **权限缓存**: 高性能的权限查询和验证
//! - **审计日志**: 完整的权限操作审计
//!
//! ## 使用示例
//!
//! ```rust
//! use bey_permissions::{PermissionManager, Role, Permission};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = PermissionManager::new().await?;
//!
//! // 创建角色
//! let admin_role = manager.create_role("admin", "管理员").await?;
//!
//! // 分配权限
//! manager.grant_permission(&admin_role, Permission::FileTransfer).await?;
//!
//! // 分配角色给用户
//! manager.assign_role("user-001", &admin_role).await?;
//!
//! // 检查权限
//! let has_permission = manager.check_permission("user-001", Permission::FileTransfer).await?;
//! println!("用户有文件传输权限: {}", has_permission);
//! # Ok(())
//! # }
//! ```

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{info, debug};

/// 权限管理结果类型
pub type PermissionResult<T> = std::result::Result<T, ErrorInfo>;

/// 权限枚举
///
/// 定义系统中所有可用的权限类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    // 文件传输权限 - 细粒度控制
    FileUpload,
    FileDownload,
    FileDelete,
    FileList,
    FileShare,           // 文件分享权限
    FileTransferCreate,   // 创建传输任务权限
    FileTransferPause,     // 暂停传输任务权限
    FileTransferResume,    // 恢复传输任务权限
    FileTransferCancel,    // 取消传输任务权限
    FileTransferMonitor,   // 监控传输任务权限
    FileTransferHistory,   // 查看传输历史权限

    // 剪切板权限
    ClipboardRead,
    ClipboardWrite,

    // 消息权限
    MessageSend,
    MessageReceive,
    MessageBroadcast,

    // 设备管理权限
    DeviceManage,
    DeviceDiscover,
    DeviceConnect,
    DeviceDisconnect,

    // 存储权限
    StorageContribute,
    StorageUse,
    StorageManage,

    // 证书权限
    CertificateManage,
    CertificateVerify,

    // 用户管理权限
    UserManage,
    RoleManage,
    PermissionManage,

    // 系统权限
    SystemConfigure,
    SystemMonitor,
    SystemLog,
}

impl Permission {
    /// 获取权限的描述
    pub fn description(&self) -> &'static str {
        match self {
            Permission::FileUpload => "文件上传权限",
            Permission::FileDownload => "文件下载权限",
            Permission::FileDelete => "文件删除权限",
            Permission::FileList => "文件列表权限",
            Permission::FileShare => "文件分享权限",
            Permission::FileTransferCreate => "创建传输任务权限",
            Permission::FileTransferPause => "暂停传输任务权限",
            Permission::FileTransferResume => "恢复传输任务权限",
            Permission::FileTransferCancel => "取消传输任务权限",
            Permission::FileTransferMonitor => "监控传输任务权限",
            Permission::FileTransferHistory => "查看传输历史权限",
            Permission::ClipboardRead => "剪切板读取权限",
            Permission::ClipboardWrite => "剪切板写入权限",
            Permission::MessageSend => "消息发送权限",
            Permission::MessageReceive => "消息接收权限",
            Permission::MessageBroadcast => "消息广播权限",
            Permission::DeviceManage => "设备管理权限",
            Permission::DeviceDiscover => "设备发现权限",
            Permission::DeviceConnect => "设备连接权限",
            Permission::DeviceDisconnect => "设备断开权限",
            Permission::StorageContribute => "存储贡献权限",
            Permission::StorageUse => "存储使用权限",
            Permission::StorageManage => "存储管理权限",
            Permission::CertificateManage => "证书管理权限",
            Permission::CertificateVerify => "证书验证权限",
            Permission::UserManage => "用户管理权限",
            Permission::RoleManage => "角色管理权限",
            Permission::PermissionManage => "权限管理权限",
            Permission::SystemConfigure => "系统配置权限",
            Permission::SystemMonitor => "系统监控权限",
            Permission::SystemLog => "系统日志权限",
        }
    }

    /// 获取权限的分类
    pub fn category(&self) -> PermissionCategory {
        match self {
            Permission::FileUpload | Permission::FileDownload |
            Permission::FileDelete | Permission::FileList |
            Permission::FileShare | Permission::FileTransferCreate |
            Permission::FileTransferPause | Permission::FileTransferResume |
            Permission::FileTransferCancel | Permission::FileTransferMonitor |
            Permission::FileTransferHistory => PermissionCategory::File,

            Permission::ClipboardRead | Permission::ClipboardWrite => PermissionCategory::Clipboard,

            Permission::MessageSend | Permission::MessageReceive |
            Permission::MessageBroadcast => PermissionCategory::Message,

            Permission::DeviceManage | Permission::DeviceDiscover |
            Permission::DeviceConnect | Permission::DeviceDisconnect => PermissionCategory::Device,

            Permission::StorageContribute | Permission::StorageUse |
            Permission::StorageManage => PermissionCategory::Storage,

            Permission::CertificateManage | Permission::CertificateVerify => PermissionCategory::Certificate,

            Permission::UserManage | Permission::RoleManage |
            Permission::PermissionManage => PermissionCategory::User,

            Permission::SystemConfigure | Permission::SystemMonitor |
            Permission::SystemLog => PermissionCategory::System,
        }
    }
}

/// 权限分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionCategory {
    File,
    Clipboard,
    Message,
    Device,
    Storage,
    Certificate,
    User,
    System,
}

/// 角色结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// 角色ID
    pub role_id: String,
    /// 角色名称
    pub name: String,
    /// 角色描述
    pub description: String,
    /// 角色权限集合
    pub permissions: HashSet<Permission>,
    /// 父角色ID（用于角色继承）
    pub parent_role_id: Option<String>,
    /// 创建时间
    pub created_at: SystemTime,
    /// 更新时间
    pub updated_at: SystemTime,
    /// 是否启用
    pub enabled: bool,
}

/// 用户角色关联
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRole {
    /// 用户ID
    pub user_id: String,
    /// 角色ID
    pub role_id: String,
    /// 分配时间
    pub assigned_at: SystemTime,
    /// 分配者
    pub assigned_by: String,
    /// 过期时间（可选）
    pub expires_at: Option<SystemTime>,
    /// 是否启用
    pub enabled: bool,
}

/// 权限审计日志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionAuditLog {
    /// 日志ID
    pub log_id: String,
    /// 操作类型
    pub operation: AuditOperation,
    /// 操作者
    pub operator: String,
    /// 目标用户/角色
    pub target: String,
    /// 涉及的权限
    pub permission: Option<Permission>,
    /// 操作时间
    pub timestamp: SystemTime,
    /// 操作结果
    pub result: AuditResult,
    /// 操作描述
    pub description: String,
}

/// 审计操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditOperation {
    GrantPermission,
    RevokePermission,
    AssignRole,
    UnassignRole,
    CreateRole,
    DeleteRole,
    UpdateRole,
    CheckPermission,
}

/// 审计结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure,
    Denied,
}

/// 权限管理器
///
/// 提供完整的权限管理功能
pub struct PermissionManager {
    /// 角色存储
    roles: Arc<RwLock<HashMap<String, Role>>>,
    /// 用户角色关联
    user_roles: Arc<RwLock<HashMap<String, Vec<UserRole>>>>,
    /// 权限缓存（用户ID -> 权限集合）
    permission_cache: Arc<RwLock<HashMap<String, HashSet<Permission>>>>,
    /// 审计日志
    audit_logs: Arc<RwLock<Vec<PermissionAuditLog>>>,
    /// 配置
    config: PermissionConfig,
}

/// 权限管理器配置
#[derive(Debug, Clone)]
pub struct PermissionConfig {
    /// 是否启用审计日志
    enable_audit: bool,
    /// 审计日志保留天数
    audit_retention_days: u32,
    /// 权限缓存过期时间（秒）
    #[allow(dead_code)]
    cache_ttl: u64,
    /// 最大审计日志数量
    max_audit_logs: usize,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            enable_audit: true,
            audit_retention_days: 30,
            cache_ttl: 300, // 5分钟
            max_audit_logs: 10000,
        }
    }
}

impl PermissionManager {
    /// 创建新的权限管理器
    pub async fn new() -> PermissionResult<Self> {
        Self::with_config(PermissionConfig::default()).await
    }

    /// 使用配置创建权限管理器
    pub async fn with_config(config: PermissionConfig) -> PermissionResult<Self> {
        let manager = Self {
            roles: Arc::new(RwLock::new(HashMap::new())),
            user_roles: Arc::new(RwLock::new(HashMap::new())),
            permission_cache: Arc::new(RwLock::new(HashMap::new())),
            audit_logs: Arc::new(RwLock::new(Vec::new())),
            config,
        };

        // 初始化默认角色
        manager.initialize_default_roles().await?;

        info!("权限管理器初始化完成");
        Ok(manager)
    }

    /// 创建角色
    pub async fn create_role(&self, name: &str, description: &str) -> PermissionResult<Role> {
        let role_id = uuid::Uuid::new_v4().to_string();

        let role = Role {
            role_id: role_id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            permissions: HashSet::new(),
            parent_role_id: None,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            enabled: true,
        };

        {
            let mut roles = self.roles.write().await;
            roles.insert(role_id.clone(), role.clone());
        }

        // 记录审计日志
        self.audit_log(
            AuditOperation::CreateRole,
            "system",
            &role_id,
            None,
            AuditResult::Success,
            format!("创建角色: {}", name),
        ).await;

        info!("角色创建成功: {} ({})", name, role_id);
        Ok(role)
    }

    /// 删除角色
    pub async fn delete_role(&self, role_id: &str, operator: &str) -> PermissionResult<()> {
        // 检查角色是否存在
        let role_exists = {
            let roles = self.roles.read().await;
            roles.contains_key(role_id)
        };

        if !role_exists {
            return Err(ErrorInfo::new(6001, format!("角色不存在: {}", role_id))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error));
        }

        // 删除角色
        {
            let mut roles = self.roles.write().await;
            roles.remove(role_id);
        }

        // 清理相关的用户角色关联
        {
            let mut user_roles = self.user_roles.write().await;
            for (_, user_role_list) in user_roles.iter_mut() {
                user_role_list.retain(|ur| ur.role_id != role_id);
            }
        }

        // 清理权限缓存
        self.clear_permission_cache().await;

        // 记录审计日志
        self.audit_log(
            AuditOperation::DeleteRole,
            operator,
            role_id,
            None,
            AuditResult::Success,
            format!("删除角色: {}", role_id),
        ).await;

        info!("角色删除成功: {}", role_id);
        Ok(())
    }

    /// 授予权限给角色
    pub async fn grant_permission_to_role(
        &self,
        role_id: &str,
        permission: Permission,
        operator: &str,
    ) -> PermissionResult<()> {
        // 检查角色是否存在
        let mut role = {
            let mut roles = self.roles.write().await;
            let role = roles.get_mut(role_id)
                .ok_or_else(|| ErrorInfo::new(6002, format!("角色不存在: {}", role_id))
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Error))?;
            role.permissions.insert(permission);
            role.clone()
        };

        role.updated_at = SystemTime::now();

        // 更新角色
        {
            let mut roles = self.roles.write().await;
            roles.insert(role_id.to_string(), role);
        }

        // 清理权限缓存
        self.clear_permission_cache().await;

        // 记录审计日志
        self.audit_log(
            AuditOperation::GrantPermission,
            operator,
            role_id,
            Some(permission),
            AuditResult::Success,
            format!("角色 {} 获得权限: {:?}", role_id, permission),
        ).await;

        info!("角色 {} 获得权限: {:?}", role_id, permission);
        Ok(())
    }

    /// 从角色撤销权限
    pub async fn revoke_permission_from_role(
        &self,
        role_id: &str,
        permission: Permission,
        operator: &str,
    ) -> PermissionResult<()> {
        // 检查角色是否存在
        let mut role = {
            let mut roles = self.roles.write().await;
            let role = roles.get_mut(role_id)
                .ok_or_else(|| ErrorInfo::new(6003, format!("角色不存在: {}", role_id))
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Error))?;
            role.permissions.remove(&permission);
            role.clone()
        };

        role.updated_at = SystemTime::now();

        // 更新角色
        {
            let mut roles = self.roles.write().await;
            roles.insert(role_id.to_string(), role);
        }

        // 清理权限缓存
        self.clear_permission_cache().await;

        // 记录审计日志
        self.audit_log(
            AuditOperation::RevokePermission,
            operator,
            role_id,
            Some(permission),
            AuditResult::Success,
            format!("角色 {} 失去权限: {:?}", role_id, permission),
        ).await;

        info!("角色 {} 失去权限: {:?}", role_id, permission);
        Ok(())
    }

    /// 分配角色给用户
    pub async fn assign_role_to_user(
        &self,
        user_id: &str,
        role_id: &str,
        operator: &str,
    ) -> PermissionResult<()> {
        // 检查角色是否存在
        {
            let roles = self.roles.read().await;
            if !roles.contains_key(role_id) {
                return Err(ErrorInfo::new(6004, format!("角色不存在: {}", role_id))
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Error));
            }
        }

        // 创建用户角色关联
        let user_role = UserRole {
            user_id: user_id.to_string(),
            role_id: role_id.to_string(),
            assigned_at: SystemTime::now(),
            assigned_by: operator.to_string(),
            expires_at: None,
            enabled: true,
        };

        // 添加到用户角色关联
        {
            let mut user_roles = self.user_roles.write().await;
            let role_list = user_roles.entry(user_id.to_string()).or_insert_with(Vec::new);

            // 检查是否已经分配了该角色
            if role_list.iter().any(|ur| ur.role_id == role_id && ur.enabled) {
                return Err(ErrorInfo::new(6005, format!("用户 {} 已经拥有角色 {}", user_id, role_id))
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Warning));
            }

            role_list.push(user_role);
        }

        // 清理权限缓存
        self.clear_permission_cache().await;

        // 记录审计日志
        self.audit_log(
            AuditOperation::AssignRole,
            operator,
            user_id,
            None,
            AuditResult::Success,
            format!("用户 {} 获得角色: {}", user_id, role_id),
        ).await;

        info!("用户 {} 获得角色: {}", user_id, role_id);
        Ok(())
    }

    /// 从用户撤销角色
    pub async fn unassign_role_from_user(
        &self,
        user_id: &str,
        role_id: &str,
        operator: &str,
    ) -> PermissionResult<()> {
        // 从用户角色关联中移除
        let mut found = false;
        {
            let mut user_roles = self.user_roles.write().await;
            if let Some(role_list) = user_roles.get_mut(user_id) {
                for user_role in role_list.iter_mut() {
                    if user_role.role_id == role_id && user_role.enabled {
                        user_role.enabled = false;
                        found = true;
                        break;
                    }
                }
            }
        }

        if !found {
            return Err(ErrorInfo::new(6006, format!("用户 {} 没有角色 {}", user_id, role_id))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Warning));
        }

        // 清理权限缓存
        self.clear_permission_cache().await;

        // 记录审计日志
        self.audit_log(
            AuditOperation::UnassignRole,
            operator,
            user_id,
            None,
            AuditResult::Success,
            format!("用户 {} 失去角色: {}", user_id, role_id),
        ).await;

        info!("用户 {} 失去角色: {}", user_id, role_id);
        Ok(())
    }

    /// 检查用户权限
    pub async fn check_permission(&self, user_id: &str, permission: Permission) -> PermissionResult<bool> {
        // 首先检查缓存
        {
            let cache = self.permission_cache.read().await;
            if let Some(user_permissions) = cache.get(user_id) {
                let has_permission = user_permissions.contains(&permission);

                // 记录审计日志
                self.audit_log(
                    AuditOperation::CheckPermission,
                    user_id,
                    user_id,
                    Some(permission),
                    if has_permission { AuditResult::Success } else { AuditResult::Denied },
                    format!("权限检查: {:?} -> {}", permission, has_permission),
                ).await;

                return Ok(has_permission);
            }
        }

        // 缓存未命中，计算用户权限
        let user_permissions = self.calculate_user_permissions(user_id).await?;
        let has_permission = user_permissions.contains(&permission);

        // 更新缓存
        {
            let mut cache = self.permission_cache.write().await;
            cache.insert(user_id.to_string(), user_permissions.clone());
        }

        // 记录审计日志
        self.audit_log(
            AuditOperation::CheckPermission,
            user_id,
            user_id,
            Some(permission),
            if has_permission { AuditResult::Success } else { AuditResult::Denied },
            format!("权限检查: {:?} -> {}", permission, has_permission),
        ).await;

        Ok(has_permission)
    }

    /// 获取用户所有权限
    pub async fn get_user_permissions(&self, user_id: &str) -> PermissionResult<HashSet<Permission>> {
        // 检查缓存
        {
            let cache = self.permission_cache.read().await;
            if let Some(user_permissions) = cache.get(user_id) {
                return Ok(user_permissions.clone());
            }
        }

        // 计算用户权限
        let user_permissions = self.calculate_user_permissions(user_id).await?;

        // 更新缓存
        {
            let mut cache = self.permission_cache.write().await;
            cache.insert(user_id.to_string(), user_permissions.clone());
        }

        Ok(user_permissions)
    }

    /// 获取用户角色
    pub async fn get_user_roles(&self, user_id: &str) -> PermissionResult<Vec<Role>> {
        let user_role_ids = {
            let user_roles = self.user_roles.read().await;
            if let Some(role_list) = user_roles.get(user_id) {
                role_list.iter()
                    .filter(|ur| ur.enabled)
                    .map(|ur| ur.role_id.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        };

        let roles = {
            let roles = self.roles.read().await;
            user_role_ids.iter()
                .filter_map(|role_id| roles.get(role_id).cloned())
                .collect()
        };

        Ok(roles)
    }

    /// 列出所有角色
    pub async fn list_roles(&self) -> PermissionResult<Vec<Role>> {
        let roles = self.roles.read().await;
        Ok(roles.values().cloned().collect())
    }

    /// 获取审计日志
    pub async fn get_audit_logs(&self, limit: Option<usize>) -> PermissionResult<Vec<PermissionAuditLog>> {
        let logs = self.audit_logs.read().await;
        let limit = limit.unwrap_or(100);

        let mut sorted_logs = logs.clone();
        sorted_logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(sorted_logs.into_iter().take(limit).collect())
    }

    /// 清理过期的审计日志
    pub async fn cleanup_audit_logs(&self) -> PermissionResult<usize> {
        let cutoff_time = SystemTime::now() - Duration::from_secs(self.config.audit_retention_days as u64 * 86400);

        let mut logs = self.audit_logs.write().await;
        let initial_count = logs.len();
        logs.retain(|log| log.timestamp > cutoff_time);
        let removed_count = initial_count - logs.len();

        // 如果日志数量仍然超过限制，删除最旧的日志
        if logs.len() > self.config.max_audit_logs {
            logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            logs.truncate(self.config.max_audit_logs);
        }

        info!("清理审计日志完成，删除 {} 条记录", removed_count);
        Ok(removed_count)
    }

    /// 计算用户权限
    async fn calculate_user_permissions(&self, user_id: &str) -> PermissionResult<HashSet<Permission>> {
        let mut user_permissions = HashSet::new();

        // 获取用户的所有角色
        let user_role_ids = {
            let user_roles = self.user_roles.read().await;
            if let Some(role_list) = user_roles.get(user_id) {
                role_list.iter()
                    .filter(|ur| ur.enabled && Self::is_role_valid(ur))
                    .map(|ur| ur.role_id.clone())
                    .collect::<HashSet<_>>()
            } else {
                HashSet::new()
            }
        };

        // 收集所有角色的权限
        {
            let roles = self.roles.read().await;
            for role_id in user_role_ids {
                if let Some(role) = roles.get(&role_id) {
                    if role.enabled {
                        user_permissions.extend(&role.permissions);

                        // 处理角色继承
                        if let Some(parent_id) = &role.parent_role_id {
                            if let Some(parent_role) = roles.get(parent_id) {
                                if parent_role.enabled {
                                    user_permissions.extend(&parent_role.permissions);
                                }
                            }
                        }
                    }
                }
            }
        }

        debug!("用户 {} 的权限: {:?}", user_id, user_permissions);
        Ok(user_permissions)
    }

    /// 检查角色是否有效
    fn is_role_valid(user_role: &UserRole) -> bool {
        if let Some(expires_at) = user_role.expires_at {
            SystemTime::now() < expires_at
        } else {
            true
        }
    }

    /// 清理权限缓存
    async fn clear_permission_cache(&self) {
        let mut cache = self.permission_cache.write().await;
        cache.clear();
        debug!("权限缓存已清理");
    }

    /// 记录审计日志
    async fn audit_log(
        &self,
        operation: AuditOperation,
        operator: &str,
        target: &str,
        permission: Option<Permission>,
        result: AuditResult,
        description: String,
    ) {
        if !self.config.enable_audit {
            return;
        }

        let log = PermissionAuditLog {
            log_id: uuid::Uuid::new_v4().to_string(),
            operation,
            operator: operator.to_string(),
            target: target.to_string(),
            permission,
            timestamp: SystemTime::now(),
            result,
            description,
        };

        let mut logs = self.audit_logs.write().await;
        logs.push(log);

        // 如果日志数量超过限制，删除最旧的日志
        if logs.len() > self.config.max_audit_logs {
            logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            logs.truncate(self.config.max_audit_logs);
        }
    }

    /// 初始化默认角色
    async fn initialize_default_roles(&self) -> PermissionResult<()> {
        // 创建管理员角色
        let admin_role = self.create_role("admin", "系统管理员").await?;
        let admin_permissions = vec![
            Permission::FileUpload, Permission::FileDownload, Permission::FileDelete, Permission::FileList,
            Permission::ClipboardRead, Permission::ClipboardWrite,
            Permission::MessageSend, Permission::MessageReceive, Permission::MessageBroadcast,
            Permission::DeviceManage, Permission::DeviceDiscover, Permission::DeviceConnect, Permission::DeviceDisconnect,
            Permission::StorageContribute, Permission::StorageUse, Permission::StorageManage,
            Permission::CertificateManage, Permission::CertificateVerify,
            Permission::UserManage, Permission::RoleManage, Permission::PermissionManage,
            Permission::SystemConfigure, Permission::SystemMonitor, Permission::SystemLog,
        ];

        for permission in admin_permissions {
            let _ = self.grant_permission_to_role(&admin_role.role_id, permission, "system").await;
        }

        // 创建普通用户角色
        let user_role = self.create_role("user", "普通用户").await?;
        let user_permissions = vec![
            Permission::FileUpload, Permission::FileDownload, Permission::FileList,
            Permission::ClipboardRead, Permission::ClipboardWrite,
            Permission::MessageSend, Permission::MessageReceive,
            Permission::DeviceDiscover, Permission::DeviceConnect,
            Permission::StorageUse,
            Permission::SystemMonitor,
        ];

        for permission in user_permissions {
            let _ = self.grant_permission_to_role(&user_role.role_id, permission, "system").await;
        }

        // 创建访客角色
        let guest_role = self.create_role("guest", "访客").await?;
        let guest_permissions = vec![
            Permission::DeviceDiscover,
            Permission::SystemMonitor,
        ];

        for permission in guest_permissions {
            let _ = self.grant_permission_to_role(&guest_role.role_id, permission, "system").await;
        }

        info!("默认角色初始化完成: admin, user, guest");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_permission_manager() -> PermissionResult<PermissionManager> {
        let config = PermissionConfig {
            enable_audit: true,
            audit_retention_days: 7,
            cache_ttl: 60,
            max_audit_logs: 100,
        };
        PermissionManager::with_config(config).await
    }

    #[tokio::test]
    async fn test_permission_manager_creation() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        // 验证默认角色已创建
        let roles = manager.list_roles().await?;
        assert!(roles.len() >= 3, "应该至少有3个默认角色");

        let role_names: HashSet<&str> = roles.iter().map(|r| r.name.as_str()).collect();
        assert!(role_names.contains("admin"), "应该有管理员角色");
        assert!(role_names.contains("user"), "应该有用户角色");
        assert!(role_names.contains("guest"), "应该有访客角色");

        Ok(())
    }

    #[tokio::test]
    async fn test_role_creation() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        let role = manager.create_role("test_role", "测试角色").await?;
        assert_eq!(role.name, "test_role");
        assert_eq!(role.description, "测试角色");
        assert!(role.enabled);
        assert!(role.permissions.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_permission_grant_revoke() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        let role = manager.create_role("test_role", "测试角色").await?;

        // 授予权限
        manager.grant_permission_to_role(&role.role_id, Permission::FileUpload, "test_operator").await?;

        // 验证权限已授予
        let updated_roles = manager.list_roles().await?;
        let updated_role = updated_roles.iter().find(|r| r.role_id == role.role_id).unwrap();
        assert!(updated_role.permissions.contains(&Permission::FileUpload));

        // 撤销权限
        manager.revoke_permission_from_role(&role.role_id, Permission::FileUpload, "test_operator").await?;

        // 验证权限已撤销
        let final_roles = manager.list_roles().await?;
        let final_role = final_roles.iter().find(|r| r.role_id == role.role_id).unwrap();
        assert!(!final_role.permissions.contains(&Permission::FileUpload));

        Ok(())
    }

    #[tokio::test]
    async fn test_role_assignment() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        let roles = manager.list_roles().await?;
        let user_role = roles.iter().find(|r| r.name == "user").unwrap();

        // 分配角色给用户
        manager.assign_role_to_user("test_user", &user_role.role_id, "test_operator").await?;

        // 验证用户有角色
        let user_roles = manager.get_user_roles("test_user").await?;
        assert_eq!(user_roles.len(), 1);
        assert_eq!(user_roles[0].role_id, user_role.role_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_permission_check() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        let roles = manager.list_roles().await?;
        let user_role = roles.iter().find(|r| r.name == "user").unwrap();

        // 分配角色给用户
        manager.assign_role_to_user("test_user", &user_role.role_id, "test_operator").await?;

        // 检查用户权限
        let can_upload = manager.check_permission("test_user", Permission::FileUpload).await?;
        assert!(can_upload, "用户应该有文件上传权限");

        let can_manage_users = manager.check_permission("test_user", Permission::UserManage).await?;
        assert!(!can_manage_users, "普通用户不应该有用户管理权限");

        Ok(())
    }

    #[tokio::test]
    async fn test_permission_inheritance() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        // 创建父角色
        let parent_role = manager.create_role("parent", "父角色").await?;
        manager.grant_permission_to_role(&parent_role.role_id, Permission::FileUpload, "test_operator").await?;

        // 创建子角色
        let mut child_role = manager.create_role("child", "子角色").await?;
        child_role.parent_role_id = Some(parent_role.role_id.clone());

        // 更新子角色
        {
            let mut roles = manager.roles.write().await;
            roles.insert(child_role.role_id.clone(), child_role.clone());
        }

        // 分配子角色给用户
        manager.assign_role_to_user("test_user", &child_role.role_id, "test_operator").await?;

        // 检查用户是否继承了父角色的权限
        let can_upload = manager.check_permission("test_user", Permission::FileUpload).await?;
        assert!(can_upload, "用户应该继承父角色的文件上传权限");

        Ok(())
    }

    #[tokio::test]
    async fn test_audit_logs() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        // 执行一些操作
        let role = manager.create_role("audit_test", "审计测试").await?;
        manager.grant_permission_to_role(&role.role_id, Permission::FileUpload, "test_operator").await?;
        manager.assign_role_to_user("test_user", &role.role_id, "test_operator").await?;

        // 检查审计日志
        let logs = manager.get_audit_logs(Some(10)).await?;
        assert!(!logs.is_empty(), "应该有审计日志");

        // 验证日志内容
        let role_creation_logs: Vec<_> = logs.iter()
            .filter(|log| log.operation == AuditOperation::CreateRole)
            .collect();
        assert!(!role_creation_logs.is_empty(), "应该有角色创建日志");

        Ok(())
    }

    #[tokio::test]
    async fn test_permission_caching() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        let roles = manager.list_roles().await?;
        let user_role = roles.iter().find(|r| r.name == "user").unwrap();

        // 分配角色给用户
        manager.assign_role_to_user("test_user", &user_role.role_id, "test_operator").await?;

        // 第一次权限检查（应该缓存结果）
        let result1 = manager.check_permission("test_user", Permission::FileUpload).await?;

        // 第二次权限检查（应该使用缓存）
        let result2 = manager.check_permission("test_user", Permission::FileUpload).await?;

        assert_eq!(result1, result2, "两次检查结果应该一致");
        assert!(result1, "用户应该有权限");

        Ok(())
    }

    #[tokio::test]
    async fn test_role_deletion() -> PermissionResult<()> {
        let manager = create_test_permission_manager().await?;

        let role = manager.create_role("delete_test", "删除测试").await?;
        let role_id = role.role_id.clone();

        // 分配角色给用户
        manager.assign_role_to_user("test_user", &role_id, "test_operator").await?;

        // 删除角色
        manager.delete_role(&role_id, "test_operator").await?;

        // 验证角色已删除
        let roles = manager.list_roles().await?;
        assert!(!roles.iter().any(|r| r.role_id == role_id), "角色应该已删除");

        // 验证用户角色关联已清理
        let user_roles = manager.get_user_roles("test_user").await?;
        assert!(!user_roles.iter().any(|r| r.role_id == role_id), "用户角色关联应该已清理");

        Ok(())
    }
}

// 导出策略引擎模块
mod policy_engine;
pub use policy_engine::{
    PolicyEngine, PolicyEngineConfig, PolicyRule, PolicyCondition, PolicyValue,
    PolicyOperator, PolicyEffect, PolicyRequest, PolicyDecision, PolicyEngineStats
};