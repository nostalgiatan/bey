//! # 插件生命周期管理模块
//!
//! 定义插件的状态和元数据

use serde::{Serialize, Deserialize};

/// 插件状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    /// 未加载
    Unloaded,
    /// 已初始化
    Initialized,
    /// 正在运行
    Running,
    /// 已停止
    Stopped,
    /// 发生错误
    Error,
}

/// 插件元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// 插件名称
    pub name: String,
    /// 插件版本
    pub version: String,
    /// 插件描述
    pub description: String,
    /// 插件作者
    pub author: String,
    /// 插件依赖列表
    pub dependencies: Vec<String>,
}

impl PluginMetadata {
    /// 创建新的插件元数据
    pub fn new(name: String, version: String) -> Self {
        Self {
            name,
            version,
            description: String::new(),
            author: String::new(),
            dependencies: Vec::new(),
        }
    }
    
    /// 设置描述
    pub fn with_description(mut self, description: String) -> Self {
        self.description = description;
        self
    }
    
    /// 设置作者
    pub fn with_author(mut self, author: String) -> Self {
        self.author = author;
        self
    }
    
    /// 添加依赖
    pub fn with_dependency(mut self, dependency: String) -> Self {
        self.dependencies.push(dependency);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_plugin_metadata_creation() {
        let metadata = PluginMetadata::new("test".to_string(), "1.0.0".to_string())
            .with_description("测试插件".to_string())
            .with_author("Test Author".to_string())
            .with_dependency("dep1".to_string());
        
        assert_eq!(metadata.name, "test");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.description, "测试插件");
        assert_eq!(metadata.author, "Test Author");
        assert_eq!(metadata.dependencies.len(), 1);
    }
}
