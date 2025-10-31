//! # 策略上下文模块
//!
//! 定义策略评估的上下文环境

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

/// 策略上下文
///
/// 包含策略评估所需的所有上下文信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    /// 上下文数据字段
    pub data: HashMap<String, serde_json::Value>,
    /// 请求者ID
    pub requester_id: Option<String>,
    /// 目标资源
    pub resource: Option<String>,
    /// 操作类型
    pub operation: Option<String>,
    /// 时间戳
    pub timestamp: SystemTime,
    /// 上下文标签
    pub tags: HashSet<String>,
}

impl PolicyContext {
    /// 创建新的策略上下文
    ///
    /// # 返回值
    ///
    /// 返回一个空的策略上下文
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            requester_id: None,
            resource: None,
            operation: None,
            timestamp: SystemTime::now(),
            tags: HashSet::new(),
        }
    }

    /// 设置字段值
    ///
    /// # 参数
    ///
    /// * `field` - 字段名
    /// * `value` - 字段值
    ///
    /// # 返回值
    ///
    /// 返回修改后的上下文（支持链式调用）
    pub fn set_field(mut self, field: String, value: serde_json::Value) -> Self {
        self.data.insert(field, value);
        self
    }

    /// 获取字段值
    ///
    /// # 参数
    ///
    /// * `field` - 字段名
    ///
    /// # 返回值
    ///
    /// 返回字段值的引用，如果字段不存在则返回None
    pub fn get_field_value(&self, field: &str) -> Option<&serde_json::Value> {
        self.data.get(field)
    }

    /// 设置请求者ID
    ///
    /// # 参数
    ///
    /// * `requester_id` - 请求者的唯一标识
    ///
    /// # 返回值
    ///
    /// 返回修改后的上下文（支持链式调用）
    pub fn with_requester_id(mut self, requester_id: String) -> Self {
        self.requester_id = Some(requester_id);
        self
    }

    /// 设置目标资源
    ///
    /// # 参数
    ///
    /// * `resource` - 目标资源标识
    ///
    /// # 返回值
    ///
    /// 返回修改后的上下文（支持链式调用）
    pub fn with_resource(mut self, resource: String) -> Self {
        self.resource = Some(resource);
        self
    }

    /// 设置操作类型
    ///
    /// # 参数
    ///
    /// * `operation` - 操作类型（如：read、write、delete等）
    ///
    /// # 返回值
    ///
    /// 返回修改后的上下文（支持链式调用）
    pub fn with_operation(mut self, operation: String) -> Self {
        self.operation = Some(operation);
        self
    }

    /// 添加标签
    ///
    /// # 参数
    ///
    /// * `tag` - 标签字符串
    ///
    /// # 返回值
    ///
    /// 返回修改后的上下文（支持链式调用）
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tags.insert(tag);
        self
    }
}

impl Default for PolicyContext {
    fn default() -> Self {
        Self::new()
    }
}
