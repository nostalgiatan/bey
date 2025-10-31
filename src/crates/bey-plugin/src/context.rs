//! # 插件上下文模块
//!
//! 为插件提供运行时环境和 API 访问

use std::sync::Arc;
use dashmap::DashMap;
use crate::{EventBus, HookRegistry};

/// 插件上下文
///
/// 为插件提供访问系统资源和其他插件的能力
pub struct PluginContext {
    /// 插件名称
    plugin_name: String,
    /// 事件总线引用
    event_bus: Arc<EventBus>,
    /// 钩子注册表引用
    hook_registry: Arc<HookRegistry>,
    /// 插件数据存储
    data: DashMap<String, Vec<u8>>,
}

impl PluginContext {
    /// 创建新的插件上下文
    pub fn new(
        plugin_name: String,
        event_bus: Arc<EventBus>,
        hook_registry: Arc<HookRegistry>,
    ) -> Self {
        Self {
            plugin_name,
            event_bus,
            hook_registry,
            data: DashMap::new(),
        }
    }
    
    /// 获取插件名称
    pub fn plugin_name(&self) -> &str {
        &self.plugin_name
    }
    
    /// 获取事件总线引用
    pub fn event_bus(&self) -> Arc<EventBus> {
        Arc::clone(&self.event_bus)
    }
    
    /// 获取钩子注册表引用
    pub fn hook_registry(&self) -> Arc<HookRegistry> {
        Arc::clone(&self.hook_registry)
    }
    
    /// 存储数据
    ///
    /// # 参数
    ///
    /// * `key` - 数据键
    /// * `value` - 数据值
    pub fn set_data(&self, key: String, value: Vec<u8>) {
        self.data.insert(key, value);
    }
    
    /// 获取数据
    ///
    /// # 参数
    ///
    /// * `key` - 数据键
    ///
    /// # 返回值
    ///
    /// 返回数据值，如果不存在则返回 None
    pub fn get_data(&self, key: &str) -> Option<Vec<u8>> {
        self.data.get(key).map(|v| v.clone())
    }
    
    /// 删除数据
    ///
    /// # 参数
    ///
    /// * `key` - 数据键
    ///
    /// # 返回值
    ///
    /// 返回被删除的数据值，如果不存在则返回 None
    pub fn remove_data(&self, key: &str) -> Option<Vec<u8>> {
        self.data.remove(key).map(|(_, v)| v)
    }
    
    /// 清除所有数据
    pub fn clear_data(&self) {
        self.data.clear();
    }
    
    /// 检查数据是否存在
    ///
    /// # 参数
    ///
    /// * `key` - 数据键
    ///
    /// # 返回值
    ///
    /// 如果数据存在返回 true，否则返回 false
    pub fn has_data(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_plugin_context_data_storage() {
        let event_bus = Arc::new(EventBus::new());
        let hook_registry = Arc::new(HookRegistry::new());
        let ctx = PluginContext::new("test".to_string(), event_bus, hook_registry);
        
        // 测试存储
        ctx.set_data("key1".to_string(), b"value1".to_vec());
        assert!(ctx.has_data("key1"));
        
        // 测试读取
        let value = ctx.get_data("key1");
        assert_eq!(value, Some(b"value1".to_vec()));
        
        // 测试删除
        let removed = ctx.remove_data("key1");
        assert_eq!(removed, Some(b"value1".to_vec()));
        assert!(!ctx.has_data("key1"));
    }
    
    #[test]
    fn test_plugin_context_clear() {
        let event_bus = Arc::new(EventBus::new());
        let hook_registry = Arc::new(HookRegistry::new());
        let ctx = PluginContext::new("test".to_string(), event_bus, hook_registry);
        
        ctx.set_data("key1".to_string(), b"value1".to_vec());
        ctx.set_data("key2".to_string(), b"value2".to_vec());
        
        ctx.clear_data();
        
        assert!(!ctx.has_data("key1"));
        assert!(!ctx.has_data("key2"));
    }
}
