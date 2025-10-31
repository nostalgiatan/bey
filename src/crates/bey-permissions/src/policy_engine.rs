//! # 无依赖策略引擎
//!
//! 提供高性能、无外部依赖的权限策略评估引擎
//! 支持基于规则的动态权限控制和细粒度访问管理

use error::{ErrorInfo};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

/// 策略操作符
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PolicyOperator {
    /// 等于
    Equals,
    /// 不等于
    NotEquals,
    /// 大于
    GreaterThan,
    /// 大于等于
    GreaterThanOrEqual,
    /// 小于
    LessThan,
    /// 小于等于
    LessThanOrEqual,
    /// 包含
    Contains,
    /// 不包含
    NotContains,
    /// 在列表中
    In,
    /// 不在列表中
    NotIn,
    /// 正则匹配
    Matches,
    /// 逻辑与
    And,
    /// 逻辑或
    Or,
    /// 逻辑非
    Not,
}

/// 策略值类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyValue {
    /// 字符串值
    String(String),
    /// 整数值
    Integer(i64),
    /// 浮点数值
    Float(f64),
    /// 布尔值
    Boolean(bool),
    /// 字符串列表
    StringList(Vec<String>),
    /// 时间戳
    Timestamp(SystemTime),
    /// 空值
    Null,
}

/// 策略条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCondition {
    /// 属性名
    pub attribute: String,
    /// 操作符
    pub operator: PolicyOperator,
    /// 值
    pub value: PolicyValue,
}

/// 策略规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// 规则ID
    pub rule_id: String,
    /// 规则名称
    pub name: String,
    /// 规则描述
    pub description: String,
    /// 规则优先级（数字越小优先级越高）
    pub priority: i32,
    /// 规则条件列表（AND关系）
    pub conditions: Vec<PolicyCondition>,
    /// 规则效果
    pub effect: PolicyEffect,
    /// 规则是否启用
    pub enabled: bool,
    /// 规则创建时间
    pub created_at: SystemTime,
    /// 规则更新时间
    pub updated_at: SystemTime,
    /// 规则标签
    pub tags: HashSet<String>,
}

/// 策略效果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PolicyEffect {
    /// 允许
    Allow,
    /// 拒绝
    Deny,
    /// 审计
    Audit,
}

impl std::fmt::Display for PolicyEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyEffect::Allow => write!(f, "Allow"),
            PolicyEffect::Deny => write!(f, "Deny"),
            PolicyEffect::Audit => write!(f, "Audit"),
        }
    }
}

/// 策略评估请求
#[derive(Debug, Clone)]
pub struct PolicyRequest {
    /// 请求ID
    pub request_id: String,
    /// 主体（用户/设备）ID
    pub subject_id: String,
    /// 主体类型
    pub subject_type: String,
    /// 主体属性
    pub subject_attributes: HashMap<String, PolicyValue>,
    /// 资源ID
    pub resource_id: String,
    /// 资源类型
    pub resource_type: String,
    /// 资源属性
    pub resource_attributes: HashMap<String, PolicyValue>,
    /// 操作
    pub action: String,
    /// 操作属性
    pub action_attributes: HashMap<String, PolicyValue>,
    /// 环境上下文
    pub environment: HashMap<String, PolicyValue>,
    /// 请求时间
    pub request_time: SystemTime,
}

/// 策略评估结果
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    /// 请求ID
    pub request_id: String,
    /// 决策结果
    pub effect: PolicyEffect,
    /// 匹配的规则ID列表
    pub matched_rules: Vec<String>,
    /// 决策理由
    pub reason: String,
    /// 评估耗时（微秒）
    pub evaluation_time_us: u64,
    /// 是否使用了缓存
    pub cached: bool,
    /// 评估详情
    pub details: Vec<String>,
}

/// 策略引擎配置
#[derive(Debug, Clone)]
pub struct PolicyEngineConfig {
    /// 是否启用缓存
    pub enable_cache: bool,
    /// 缓存TTL
    pub cache_ttl: Duration,
    /// 最大缓存条目数
    pub max_cache_entries: usize,
    /// 是否启用规则优先级
    pub enable_priority: bool,
    /// 默认决策（无规则匹配时）
    pub default_effect: PolicyEffect,
    /// 是否启用审计日志
    pub enable_audit_log: bool,
}

impl Default for PolicyEngineConfig {
    fn default() -> Self {
        Self {
            enable_cache: true,
            cache_ttl: Duration::from_secs(300), // 5分钟
            max_cache_entries: 10000,
            enable_priority: true,
            default_effect: PolicyEffect::Deny,
            enable_audit_log: true,
        }
    }
}

/// 策略引擎
pub struct PolicyEngine {
    /// 配置信息
    config: PolicyEngineConfig,
    /// 规则存储
    rules: Arc<RwLock<Vec<PolicyRule>>>,
    /// 策略缓存
    cache: Arc<RwLock<HashMap<String, PolicyDecision>>>,
    /// 规则索引（按优先级排序）
    rule_index: Arc<RwLock<Vec<usize>>>,
    /// 统计信息
    stats: Arc<RwLock<PolicyEngineStats>>,
}

/// 策略引擎统计信息
#[derive(Debug, Clone, Default)]
pub struct PolicyEngineStats {
    /// 总评估次数
    pub total_evaluations: u64,
    /// 缓存命中次数
    pub cache_hits: u64,
    /// 缓存未命中次数
    pub cache_misses: u64,
    /// 平均评估时间（微秒）
    pub avg_evaluation_time_us: f64,
    /// 规则匹配次数
    pub rule_matches: u64,
    /// 允许决策次数
    pub allow_decisions: u64,
    /// 拒绝决策次数
    pub deny_decisions: u64,
    /// 审计决策次数
    pub audit_decisions: u64,
}

impl PolicyEngine {
    /// 创建新的策略引擎
    ///
    /// # 参数
    ///
    /// * `config` - 策略引擎配置
    ///
    /// # 返回值
    ///
    /// 返回策略引擎实例
    pub fn new(config: PolicyEngineConfig) -> Self {
        info!("创建策略引擎，缓存: {}, 优先级: {}",
            config.enable_cache, config.enable_priority);

        Self {
            config,
            rules: Arc::new(RwLock::new(Vec::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            rule_index: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(PolicyEngineStats::default())),
        }
    }

    /// 添加策略规则
    ///
    /// # 参数
    ///
    /// * `rule` - 策略规则
    ///
    /// # 返回值
    ///
    /// 返回添加结果或错误信息
    pub async fn add_rule(&self, rule: PolicyRule) -> Result<(), ErrorInfo> {
        info!("添加策略规则: {}", rule.rule_id);

        let mut rules = self.rules.write().await;
        let _rule_index = rules.len();
        rules.push(rule.clone());

        // 重建规则索引
        self.rebuild_rule_index().await;

        // 清除缓存
        self.clear_cache().await;

        info!("策略规则添加完成: {}", rule.rule_id);
        Ok(())
    }

    /// 移除策略规则
    ///
    /// # 参数
    ///
    /// * `rule_id` - 规则ID
    ///
    /// # 返回值
    ///
    /// 返回移除结果或错误信息
    pub async fn remove_rule(&self, rule_id: &str) -> Result<bool, ErrorInfo> {
        info!("移除策略规则: {}", rule_id);

        let mut rules = self.rules.write().await;
        let original_len = rules.len();

        rules.retain(|rule| rule.rule_id != rule_id);

        if rules.len() < original_len {
            // 重建规则索引
            self.rebuild_rule_index().await;

            // 清除缓存
            self.clear_cache().await;

            info!("策略规则移除完成: {}", rule_id);
            Ok(true)
        } else {
            warn!("策略规则不存在: {}", rule_id);
            Ok(false)
        }
    }

    /// 更新策略规则
    ///
    /// # 参数
    ///
    /// * `rule` - 更新的策略规则
    ///
    /// # 返回值
    ///
    /// 返回更新结果或错误信息
    pub async fn update_rule(&self, rule: PolicyRule) -> Result<bool, ErrorInfo> {
        info!("更新策略规则: {}", rule.rule_id);

        let mut rules = self.rules.write().await;
        let mut found = false;

        for existing_rule in rules.iter_mut() {
            if existing_rule.rule_id == rule.rule_id {
                *existing_rule = rule.clone();
                found = true;
                break;
            }
        }

        if found {
            // 重建规则索引
            self.rebuild_rule_index().await;

            // 清除缓存
            self.clear_cache().await;

            info!("策略规则更新完成: {}", rule.rule_id);
        } else {
            warn!("策略规则不存在: {}", rule.rule_id);
        }

        Ok(found)
    }

    /// 获取所有规则
    ///
    /// # 返回值
    ///
    /// 返回所有策略规则
    pub async fn get_rules(&self) -> Vec<PolicyRule> {
        let rules = self.rules.read().await;
        rules.clone()
    }

    /// 获取启用的规则
    ///
    /// # 返回值
    ///
    /// 返回启用的策略规则
    pub async fn get_enabled_rules(&self) -> Vec<PolicyRule> {
        let rules = self.rules.read().await;
        rules.iter().filter(|rule| rule.enabled).cloned().collect()
    }

    /// 评估策略请求
    ///
    /// # 参数
    ///
    /// * `request` - 策略评估请求
    ///
    /// # 返回值
    ///
    /// 返回策略决策
    pub async fn evaluate(&self, request: PolicyRequest) -> PolicyDecision {
        let start_time = std::time::Instant::now();
        debug!("开始策略评估: {} -> {}", request.subject_id, request.action);

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.total_evaluations += 1;
        }

        // 检查缓存
        if self.config.enable_cache {
            if let Some(cached_decision) = self.check_cache(&request).await {
                debug!("使用缓存决策: {:?}", cached_decision.effect);

                // 更新缓存命中统计
                {
                    let mut stats = self.stats.write().await;
                    stats.cache_hits += 1;
                }

                let mut decision = cached_decision.clone();
                decision.cached = true;
                return decision;
            } else {
                // 更新缓存未命中统计
                {
                    let mut stats = self.stats.write().await;
                    stats.cache_misses += 1;
                }
            }
        }

        // 执行策略评估
        let decision = self.evaluate_internal(&request).await;

        // 计算评估时间
        let evaluation_time = start_time.elapsed().as_micros() as u64;

        // 更新评估时间统计
        {
            let mut stats = self.stats.write().await;
            let total_evaluations = stats.total_evaluations;
            stats.avg_evaluation_time_us =
                (stats.avg_evaluation_time_us * (total_evaluations - 1) as f64 + evaluation_time as f64)
                / total_evaluations as f64;
        }

        // 缓存决策结果
        if self.config.enable_cache {
            self.cache_decision(&decision).await;
        }

        // 记录决策统计
        {
            let mut stats = self.stats.write().await;
            match decision.effect {
                PolicyEffect::Allow => stats.allow_decisions += 1,
                PolicyEffect::Deny => stats.deny_decisions += 1,
                PolicyEffect::Audit => stats.audit_decisions += 1,
            }
        }

        debug!("策略评估完成: {} -> {:?}", request.action, decision.effect);
        decision
    }

    /// 获取统计信息
    ///
    /// # 返回值
    ///
    /// 返回统计信息
    pub async fn get_stats(&self) -> PolicyEngineStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// 清除缓存
    pub async fn clear_cache(&self) {
        debug!("清除策略缓存");
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    // 私有方法

    /// 重建规则索引
    async fn rebuild_rule_index(&self) {
        let rules = self.rules.read().await;
        let mut indices: Vec<usize> = (0..rules.len()).collect();

        if self.config.enable_priority {
            // 按优先级排序（数字越小优先级越高）
            indices.sort_by(|&a, &b| {
                let rule_a = &rules[a];
                let rule_b = &rules[b];

                // 启用的规则优先
                match (rule_a.enabled, rule_b.enabled) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, true) | (false, false) => {
                        // 按优先级排序
                        rule_a.priority.cmp(&rule_b.priority)
                    }
                }
            });
        }

        let mut rule_index = self.rule_index.write().await;
        *rule_index = indices.clone();
        debug!("规则索引重建完成，规则数量: {}", indices.len());
    }

    /// 检查缓存
    async fn check_cache(&self, request: &PolicyRequest) -> Option<PolicyDecision> {
        let cache_key = self.generate_cache_key(request);
        let cache = self.cache.read().await;

        cache.get(&cache_key).cloned()
    }

    /// 缓存决策结果
    async fn cache_decision(&self, decision: &PolicyDecision) {
        let cache_key = self.generate_cache_key_from_decision(decision);

        // 检查缓存大小限制
        {
            let cache = self.cache.read().await;
            if cache.len() >= self.config.max_cache_entries {
                return; // 缓存已满，跳过缓存
            }
        }

        let mut cache = self.cache.write().await;
        cache.insert(cache_key, decision.clone());
    }

    /// 生成缓存键
    fn generate_cache_key(&self, request: &PolicyRequest) -> String {
        // 简化的缓存键生成（实际应用中可能需要更复杂的逻辑）
        format!("{}:{}:{}:{}",
            request.subject_id,
            request.resource_id,
            request.action,
            request.request_time.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() / 60 // 按分钟缓存
        )
    }

    /// 从决策生成缓存键
    fn generate_cache_key_from_decision(&self, decision: &PolicyDecision) -> String {
        decision.request_id.clone()
    }

    /// 内部策略评估
    async fn evaluate_internal(&self, request: &PolicyRequest) -> PolicyDecision {
        let rules = self.rules.read().await;
        let rule_index = self.rule_index.read().await;
        let mut matched_rules = Vec::new();
        let mut details = Vec::new();
        let mut final_effect = self.config.default_effect.clone();

        // 按优先级顺序评估规则
        for &rule_idx in rule_index.iter() {
            let rule = &rules[rule_idx];

            // 跳过禁用的规则
            if !rule.enabled {
                continue;
            }

            debug!("评估规则: {} (优先级: {})", rule.rule_id, rule.priority);

            // 评估规则条件
            if self.evaluate_rule_conditions(&rule.conditions, request).await {
                matched_rules.push(rule.rule_id.clone());
                details.push(format!("规则匹配: {} - {}", rule.rule_id, rule.name));

                // 更新规则匹配统计
                {
                    let mut stats = self.stats.write().await;
                    stats.rule_matches += 1;
                }

                // 根据规则效果设置最终决策
                // 在优先级模式下，第一个匹配的规则决定最终效果
                if self.config.enable_priority {
                    final_effect = rule.effect.clone();
                    break; // 找到第一个匹配的规则就停止
                } else {
                    // 在非优先级模式下，拒绝规则优先
                    if matches!(rule.effect, PolicyEffect::Deny) {
                        final_effect = PolicyEffect::Deny;
                        break;
                    } else if matches!(rule.effect, PolicyEffect::Allow) {
                        final_effect = PolicyEffect::Allow;
                    }
                }
            } else {
                debug!("规则不匹配: {}", rule.rule_id);
            }
        }

        let reason = if matched_rules.is_empty() {
            "无匹配规则，使用默认决策".to_string()
        } else {
            format!("匹配 {} 个规则", matched_rules.len())
        };

        PolicyDecision {
            request_id: request.request_id.clone(),
            effect: final_effect,
            matched_rules,
            reason,
            evaluation_time_us: 0, // 将在调用方设置
            cached: false,
            details,
        }
    }

    /// 评估规则条件
    async fn evaluate_rule_conditions(
        &self,
        conditions: &[PolicyCondition],
        request: &PolicyRequest,
    ) -> bool {
        // 所有条件都必须满足（AND关系）
        for condition in conditions {
            if !self.evaluate_condition(condition, request).await {
                return false;
            }
        }
        true
    }

    /// 评估单个条件
    async fn evaluate_condition(&self, condition: &PolicyCondition, request: &PolicyRequest) -> bool {
        // 获取属性值
        let attribute_value = self.get_attribute_value(&condition.attribute, request);

        // 执行比较操作
        self.compare_values(&attribute_value, &condition.operator, &condition.value)
    }

    /// 获取属性值
    fn get_attribute_value(&self, attribute: &str, request: &PolicyRequest) -> PolicyValue {
        // 首先检查主体属性
        if let Some(value) = request.subject_attributes.get(attribute) {
            return value.clone();
        }

        // 检查资源属性
        if let Some(value) = request.resource_attributes.get(attribute) {
            return value.clone();
        }

        // 检查操作属性
        if let Some(value) = request.action_attributes.get(attribute) {
            return value.clone();
        }

        // 检查环境属性
        if let Some(value) = request.environment.get(attribute) {
            return value.clone();
        }

        // 检查内置属性
        match attribute {
            "subject_id" => PolicyValue::String(request.subject_id.clone()),
            "subject_type" => PolicyValue::String(request.subject_type.clone()),
            "resource_id" => PolicyValue::String(request.resource_id.clone()),
            "resource_type" => PolicyValue::String(request.resource_type.clone()),
            "action" => PolicyValue::String(request.action.clone()),
            "timestamp" => PolicyValue::Timestamp(request.request_time),
            _ => PolicyValue::Null,
        }
    }

    /// 比较两个值
    fn compare_values(&self, left: &PolicyValue, operator: &PolicyOperator, right: &PolicyValue) -> bool {
        use PolicyOperator::*;
        // use PolicyValue::*; // 未使用，注释掉

        match operator {
            Equals => self.values_equal(left, right),
            NotEquals => !self.values_equal(left, right),
            GreaterThan => self.values_greater_than(left, right),
            GreaterThanOrEqual => self.values_greater_than_or_equal(left, right),
            LessThan => self.values_less_than(left, right),
            LessThanOrEqual => self.values_less_than_or_equal(left, right),
            Contains => self.values_contains(left, right),
            NotContains => !self.values_contains(left, right),
            In => self.values_in(left, right),
            NotIn => !self.values_in(left, right),
            Matches => self.values_matches(left, right),
            And | Or | Not => {
                // 这些操作符不用于基本值比较
                false
            }
        }
    }

    /// 值相等比较
    fn values_equal(&self, left: &PolicyValue, right: &PolicyValue) -> bool {
        match (left, right) {
            (PolicyValue::String(a), PolicyValue::String(b)) => a == b,
            (PolicyValue::Integer(a), PolicyValue::Integer(b)) => a == b,
            (PolicyValue::Float(a), PolicyValue::Float(b)) => (a - b).abs() < f64::EPSILON,
            (PolicyValue::Boolean(a), PolicyValue::Boolean(b)) => a == b,
            (PolicyValue::Null, PolicyValue::Null) => true,
            _ => false,
        }
    }

    /// 值大于比较
    fn values_greater_than(&self, left: &PolicyValue, right: &PolicyValue) -> bool {
        match (left, right) {
            (PolicyValue::Integer(a), PolicyValue::Integer(b)) => a > b,
            (PolicyValue::Float(a), PolicyValue::Float(b)) => a > b,
            (PolicyValue::Timestamp(a), PolicyValue::Timestamp(b)) => a > b,
            _ => false,
        }
    }

    /// 值大于等于比较
    fn values_greater_than_or_equal(&self, left: &PolicyValue, right: &PolicyValue) -> bool {
        self.values_greater_than(left, right) || self.values_equal(left, right)
    }

    /// 值小于比较
    fn values_less_than(&self, left: &PolicyValue, right: &PolicyValue) -> bool {
        match (left, right) {
            (PolicyValue::Integer(a), PolicyValue::Integer(b)) => a < b,
            (PolicyValue::Float(a), PolicyValue::Float(b)) => a < b,
            (PolicyValue::Timestamp(a), PolicyValue::Timestamp(b)) => a < b,
            _ => false,
        }
    }

    /// 值小于等于比较
    fn values_less_than_or_equal(&self, left: &PolicyValue, right: &PolicyValue) -> bool {
        self.values_less_than(left, right) || self.values_equal(left, right)
    }

    /// 值包含比较
    fn values_contains(&self, left: &PolicyValue, right: &PolicyValue) -> bool {
        match (left, right) {
            (PolicyValue::String(text), PolicyValue::String(pattern)) => text.contains(pattern),
            (PolicyValue::StringList(list), PolicyValue::String(item)) => list.contains(item),
            _ => false,
        }
    }

    /// 值在列表中比较
    fn values_in(&self, left: &PolicyValue, right: &PolicyValue) -> bool {
        match right {
            PolicyValue::StringList(list) => {
                for item in list {
                    let item_value = PolicyValue::String(item.to_string());
                    if self.values_equal(left, &item_value) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// 正则匹配比较（简化实现）
    fn values_matches(&self, left: &PolicyValue, right: &PolicyValue) -> bool {
        match (left, right) {
            (PolicyValue::String(text), PolicyValue::String(pattern)) => {
                // 简化的匹配实现，实际应用中可能需要完整的正则表达式支持
                text.contains(&*pattern) || pattern == "*"
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[tokio::test]
    async fn test_policy_engine_creation() {
        let config = PolicyEngineConfig::default();
        let engine = PolicyEngine::new(config);

        assert_eq!(engine.config.enable_cache, true);
        assert_eq!(engine.config.default_effect, PolicyEffect::Deny);
    }

    #[tokio::test]
    async fn test_rule_addition_and_removal() {
        let engine = PolicyEngine::new(PolicyEngineConfig::default());

        let rule = PolicyRule {
            rule_id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: "Test Description".to_string(),
            priority: 100,
            conditions: vec![],
            effect: PolicyEffect::Allow,
            enabled: true,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            tags: HashSet::new(),
        };

        // 添加规则
        engine.add_rule(rule.clone()).await.unwrap();
        let rules = engine.get_rules().await;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_id, "test-rule");

        // 移除规则
        let removed = engine.remove_rule("test-rule").await.unwrap();
        assert!(removed);

        let rules = engine.get_rules().await;
        assert_eq!(rules.len(), 0);
    }

    #[tokio::test]
    async fn test_simple_policy_evaluation() {
        let engine = PolicyEngine::new(PolicyEngineConfig::default());

        // 创建允许规则：subject_id 等于 "test-user"
        let condition = PolicyCondition {
            attribute: "subject_id".to_string(),
            operator: PolicyOperator::Equals,
            value: PolicyValue::String("test-user".to_string()),
        };

        let rule = PolicyRule {
            rule_id: "allow-user".to_string(),
            name: "Allow Test User".to_string(),
            description: "Allow access for test user".to_string(),
            priority: 100,
            conditions: vec![condition],
            effect: PolicyEffect::Allow,
            enabled: true,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            tags: HashSet::new(),
        };

        engine.add_rule(rule).await.unwrap();

        // 创建评估请求
        let request = PolicyRequest {
            request_id: "test-req-1".to_string(),
            subject_id: "test-user".to_string(),
            subject_type: "user".to_string(),
            subject_attributes: HashMap::new(),
            resource_id: "test-resource".to_string(),
            resource_type: "file".to_string(),
            resource_attributes: HashMap::new(),
            action: "read".to_string(),
            action_attributes: HashMap::new(),
            environment: HashMap::new(),
            request_time: SystemTime::now(),
        };

        // 评估请求
        let decision = engine.evaluate(request).await;

        assert_eq!(decision.effect, PolicyEffect::Allow);
        assert_eq!(decision.matched_rules.len(), 1);
        assert_eq!(decision.matched_rules[0], "allow-user");
    }

    #[tokio::test]
    async fn test_policy_condition_evaluation() {
        let engine = PolicyEngine::new(PolicyEngineConfig::default());

        // 测试字符串相等
        let condition = PolicyCondition {
            attribute: "subject_id".to_string(),
            operator: PolicyOperator::Equals,
            value: PolicyValue::String("test".to_string()),
        };

        let request = PolicyRequest {
            request_id: "test".to_string(),
            subject_id: "test".to_string(),
            subject_type: "user".to_string(),
            subject_attributes: HashMap::new(),
            resource_id: "resource".to_string(),
            resource_type: "type".to_string(),
            resource_attributes: HashMap::new(),
            action: "action".to_string(),
            action_attributes: HashMap::new(),
            environment: HashMap::new(),
            request_time: SystemTime::now(),
        };

        let result = engine.evaluate_condition(&condition, &request).await;
        assert!(result);

        // 测试字符串不相等
        let condition2 = PolicyCondition {
            attribute: "subject_id".to_string(),
            operator: PolicyOperator::Equals,
            value: PolicyValue::String("other".to_string()),
        };

        let result2 = engine.evaluate_condition(&condition2, &request).await;
        assert!(!result2);
    }

    #[test]
    fn test_value_comparisons() {
        let engine = PolicyEngine::new(PolicyEngineConfig::default());

        // 字符串相等
        assert!(engine.values_equal(
            &PolicyValue::String("test".to_string()),
            &PolicyValue::String("test".to_string())
        ));

        // 整数比较
        assert!(engine.values_greater_than(
            &PolicyValue::Integer(10),
            &PolicyValue::Integer(5)
        ));

        // 字符串包含
        assert!(engine.values_contains(
            &PolicyValue::String("hello world".to_string()),
            &PolicyValue::String("world".to_string())
        ));
    }
}