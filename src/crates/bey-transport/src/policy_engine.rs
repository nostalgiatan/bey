//! # 完整的策略引擎系统
//!
//! 提供灵活、高性能的安全策略管理功能，支持动态策略更新、规则评估和决策执行。
//! 策略引擎用于控制网络连接、数据访问和系统行为的安全策略。
//!
//! ## 核心特性
//!
//! - **动态策略管理**: 支持运行时策略更新和配置变更
//! - **高性能规则评估**: 优化的规则匹配引擎，支持复杂条件判断
//! - **多维度策略**: 支持基于时间、位置、身份、资源的复合策略
//! - **策略缓存**: 智能缓存策略评估结果，提升性能
//! - **策略审计**: 完整的策略执行日志和审计追踪
//! - **灵活规则引擎**: 支持多种规则类型和逻辑运算

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{info, debug, error};

/// 策略动作类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PolicyAction {
    /// 允许操作
    Allow,
    /// 拒绝操作
    Deny,
    /// 需要额外验证
    RequireAuthentication,
    /// 限制访问
    Restrict,
    /// 记录日志
    Log,
    /// 需要审批
    RequireApproval,
    /// 隔离处理
    Quarantine,
}

/// 策略条件操作符
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConditionOperator {
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
    /// 正则匹配
    Regex,
    /// 在列表中
    In,
    /// 不在列表中
    NotIn,
    /// 逻辑与
    And,
    /// 逻辑或
    Or,
    /// 逻辑非
    Not,
}

impl std::fmt::Display for ConditionOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConditionOperator::Equals => write!(f, "=="),
            ConditionOperator::NotEquals => write!(f, "!="),
            ConditionOperator::GreaterThan => write!(f, ">"),
            ConditionOperator::GreaterThanOrEqual => write!(f, ">="),
            ConditionOperator::LessThan => write!(f, "<"),
            ConditionOperator::LessThanOrEqual => write!(f, "<="),
            ConditionOperator::Contains => write!(f, "contains"),
            ConditionOperator::NotContains => write!(f, "not_contains"),
            ConditionOperator::Regex => write!(f, "regex"),
            ConditionOperator::In => write!(f, "in"),
            ConditionOperator::NotIn => write!(f, "not_in"),
            ConditionOperator::And => write!(f, "and"),
            ConditionOperator::Or => write!(f, "or"),
            ConditionOperator::Not => write!(f, "not"),
        }
    }
}

/// 策略条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCondition {
    /// 条件字段名
    pub field: String,
    /// 操作符
    pub operator: ConditionOperator,
    /// 期望值
    pub value: serde_json::Value,
    /// 条件权重（用于复合条件计算）
    pub weight: f32,
    /// 条件描述
    pub description: String,
}

impl PolicyCondition {
    /// 创建新的策略条件
    pub fn new(
        field: String,
        operator: ConditionOperator,
        value: serde_json::Value,
        description: String,
    ) -> Self {
        Self {
            field,
            operator,
            value,
            weight: 1.0,
            description,
        }
    }

    /// 设置条件权重
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// 评估条件是否满足
    pub fn evaluate(&self, context: &PolicyContext) -> Result<bool, ErrorInfo> {
        let field_value = context.get_field_value(&self.field)
            .unwrap_or(&serde_json::Value::Null);

        let result = match (&self.operator, field_value, &self.value) {
            (ConditionOperator::Equals, actual, expected) => actual == expected,
            (ConditionOperator::NotEquals, actual, expected) => actual != expected,
            (ConditionOperator::GreaterThan, serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => {
                actual.as_f64().unwrap_or(0.0) > expected.as_f64().unwrap_or(0.0)
            }
            (ConditionOperator::GreaterThanOrEqual, serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => {
                actual.as_f64().unwrap_or(0.0) >= expected.as_f64().unwrap_or(0.0)
            }
            (ConditionOperator::LessThan, serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => {
                actual.as_f64().unwrap_or(0.0) < expected.as_f64().unwrap_or(0.0)
            }
            (ConditionOperator::LessThanOrEqual, serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => {
                actual.as_f64().unwrap_or(0.0) <= expected.as_f64().unwrap_or(0.0)
            }
            (ConditionOperator::Contains, serde_json::Value::String(actual), serde_json::Value::String(expected)) => {
                actual.contains(expected)
            }
            (ConditionOperator::NotContains, serde_json::Value::String(actual), serde_json::Value::String(expected)) => {
                !actual.contains(expected)
            }
            (ConditionOperator::In, actual, serde_json::Value::Array(expected)) => {
                expected.contains(actual)
            }
            (ConditionOperator::NotIn, actual, serde_json::Value::Array(expected)) => {
                !expected.contains(actual)
            }
            (ConditionOperator::Regex, serde_json::Value::String(actual), serde_json::Value::String(pattern)) => {
                regex::Regex::new(pattern)
                    .map_err(|e| ErrorInfo::new(6001, format!("无效的正则表达式: {}", e))
                        .with_category(ErrorCategory::Configuration)
                        .with_severity(ErrorSeverity::Error))?
                    .is_match(actual)
            }
            _ => false,
        };

        debug!("条件评估: {} {} {} = {:?}", self.field, self.operator, self.value, result);
        Ok(result)
    }
}

/// 策略规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// 规则ID
    pub id: String,
    /// 规则名称
    pub name: String,
    /// 规则描述
    pub description: String,
    /// 规则优先级（数值越大优先级越高）
    pub priority: i32,
    /// 规则条件列表
    pub conditions: Vec<PolicyCondition>,
    /// 条件组合方式（AND/OR）
    pub condition_combination: ConditionOperator,
    /// 规则动作
    pub action: PolicyAction,
    /// 规则是否启用
    pub enabled: bool,
    /// 规则创建时间
    pub created_at: SystemTime,
    /// 规则更新时间
    pub updated_at: SystemTime,
    /// 规则标签
    pub tags: HashSet<String>,
    /// 规则元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PolicyRule {
    /// 创建新的策略规则
    pub fn new(
        id: String,
        name: String,
        description: String,
        priority: i32,
        action: PolicyAction,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            id,
            name,
            description,
            priority,
            conditions: Vec::new(),
            condition_combination: ConditionOperator::And,
            action,
            enabled: true,
            created_at: now,
            updated_at: now,
            tags: HashSet::new(),
            metadata: HashMap::new(),
        }
    }

    /// 添加条件
    pub fn add_condition(mut self, condition: PolicyCondition) -> Self {
        self.conditions.push(condition);
        self.updated_at = SystemTime::now();
        self
    }

    /// 设置条件组合方式
    pub fn with_condition_combination(mut self, combination: ConditionOperator) -> Self {
        self.condition_combination = combination;
        self.updated_at = SystemTime::now();
        self
    }

    /// 添加标签
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tags.insert(tag);
        self
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self.updated_at = SystemTime::now();
        self
    }

    /// 评估规则
    pub fn evaluate(&self, context: &PolicyContext) -> Result<PolicyEvaluationResult, ErrorInfo> {
        if !self.enabled {
            return Ok(PolicyEvaluationResult {
                rule_id: self.id.clone(),
                action: PolicyAction::Allow,
                matched: false,
                score: 0.0,
                reason: "规则已禁用".to_string(),
                evaluated_conditions: Vec::new(),
                execution_time_ms: 0,
            });
        }

        let start_time = std::time::Instant::now();
        let mut evaluated_conditions = Vec::new();
        let mut total_score = 0.0;

        // 评估所有条件
        for condition in &self.conditions {
            let condition_start = std::time::Instant::now();
            let condition_result = condition.evaluate(context)?;
            let condition_time = condition_start.elapsed().as_millis() as u64;

            evaluated_conditions.push(ConditionEvaluationResult {
                condition: condition.clone(),
                result: condition_result,
                execution_time_ms: condition_time,
            });

            if condition_result {
                total_score += condition.weight;
            }
        }

        // 根据组合方式计算最终结果
        let final_result = match self.condition_combination {
            ConditionOperator::And => {
                evaluated_conditions.iter().all(|c| c.result)
            }
            ConditionOperator::Or => {
                evaluated_conditions.iter().any(|c| c.result)
            }
            _ => false,
        };

        let execution_time = start_time.elapsed().as_millis() as u64;

        Ok(PolicyEvaluationResult {
            rule_id: self.id.clone(),
            action: if final_result { self.action.clone() } else { PolicyAction::Allow },
            matched: final_result,
            score: total_score,
            reason: if final_result {
                format!("规则 '{}' 匹配成功", self.name)
            } else {
                format!("规则 '{}' 不匹配", self.name)
            },
            evaluated_conditions,
            execution_time_ms: execution_time,
        })
    }
}

/// 策略上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    /// 上下文数据
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
    pub fn set_field(mut self, field: String, value: serde_json::Value) -> Self {
        self.data.insert(field, value);
        self
    }

    /// 获取字段值
    pub fn get_field_value(&self, field: &str) -> Option<&serde_json::Value> {
        self.data.get(field)
    }

    /// 设置请求者ID
    pub fn with_requester_id(mut self, requester_id: String) -> Self {
        self.requester_id = Some(requester_id);
        self
    }

    /// 设置目标资源
    pub fn with_resource(mut self, resource: String) -> Self {
        self.resource = Some(resource);
        self
    }

    /// 设置操作类型
    pub fn with_operation(mut self, operation: String) -> Self {
        self.operation = Some(operation);
        self
    }

    /// 添加标签
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

/// 条件评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionEvaluationResult {
    /// 评估的条件
    pub condition: PolicyCondition,
    /// 评估结果
    pub result: bool,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
}

/// 策略评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluationResult {
    /// 规则ID
    pub rule_id: String,
    /// 最终动作
    pub action: PolicyAction,
    /// 是否匹配
    pub matched: bool,
    /// 评分
    pub score: f32,
    /// 评估原因
    pub reason: String,
    /// 条件评估结果列表
    pub evaluated_conditions: Vec<ConditionEvaluationResult>,
    /// 总执行时间（毫秒）
    pub execution_time_ms: u64,
}

/// 策略集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySet {
    /// 策略集合ID
    pub id: String,
    /// 策略集合名称
    pub name: String,
    /// 策略集合描述
    pub description: String,
    /// 规则列表
    pub rules: Vec<PolicyRule>,
    /// 默认动作（当没有规则匹配时）
    pub default_action: PolicyAction,
    /// 策略集合是否启用
    pub enabled: bool,
    /// 创建时间
    pub created_at: SystemTime,
    /// 更新时间
    pub updated_at: SystemTime,
    /// 策略集合标签
    pub tags: HashSet<String>,
}

impl PolicySet {
    /// 创建新的策略集合
    pub fn new(
        id: String,
        name: String,
        description: String,
        default_action: PolicyAction,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            id,
            name,
            description,
            rules: Vec::new(),
            default_action,
            enabled: true,
            created_at: now,
            updated_at: now,
            tags: HashSet::new(),
        }
    }

    /// 添加规则
    pub fn add_rule(mut self, rule: PolicyRule) -> Self {
        self.rules.push(rule);
        self.updated_at = SystemTime::now();
        self
    }

    /// 根据优先级排序规则
    pub fn sort_rules_by_priority(&mut self) {
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 评估策略集合
    pub fn evaluate(&self, context: &PolicyContext) -> Result<PolicySetEvaluationResult, ErrorInfo> {
        if !self.enabled {
            return Ok(PolicySetEvaluationResult {
                policy_set_id: self.id.clone(),
                final_action: self.default_action.clone(),
                matched_rules: Vec::new(),
                evaluation_summary: "策略集合已禁用，使用默认动作".to_string(),
                total_execution_time_ms: 0,
            });
        }

        let start_time = std::time::Instant::now();
        let mut matched_rules = Vec::new();

        // 按优先级评估规则
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            let rule_result = rule.evaluate(context)?;
            if rule_result.matched {
                matched_rules.push(rule_result.clone());

                // 如果是拒绝策略且优先级最高，可以直接返回
                if rule.action == PolicyAction::Deny && rule.priority >= 100 {
                    break;
                }
            }
        }

        // 确定最终动作
        let matched_rules_count = matched_rules.len();
        let final_action = if let Some(highest_priority_rule) = matched_rules
            .iter()
            .max_by_key(|r| self.rules.iter().find(|rule| rule.id == r.rule_id).map(|rule| rule.priority).unwrap_or(0))
        {
            highest_priority_rule.action.clone()
        } else {
            self.default_action.clone()
        };

        let execution_time = start_time.elapsed().as_millis() as u64;

        Ok(PolicySetEvaluationResult {
            policy_set_id: self.id.clone(),
            final_action,
            matched_rules,
            evaluation_summary: format!(
                "策略集合 '{}' 评估完成，匹配 {} 条规则",
                self.name,
                matched_rules_count
            ),
            total_execution_time_ms: execution_time,
        })
    }
}

/// 策略集合评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySetEvaluationResult {
    /// 策略集合ID
    pub policy_set_id: String,
    /// 最终动作
    pub final_action: PolicyAction,
    /// 匹配的规则列表
    pub matched_rules: Vec<PolicyEvaluationResult>,
    /// 评估摘要
    pub evaluation_summary: String,
    /// 总执行时间（毫秒）
    pub total_execution_time_ms: u64,
}

/// 策略引擎统计信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyEngineStats {
    /// 总评估次数
    pub total_evaluations: u64,
    /// 缓存命中次数
    pub cache_hits: u64,
    /// 缓存未命中次数
    pub cache_misses: u64,
    /// 平均评估时间（微秒）
    pub average_evaluation_time_us: u64,
    /// 最慢评估时间（微秒）
    pub slowest_evaluation_time_us: u64,
    /// 最快评估时间（微秒）
    pub fastest_evaluation_time_us: u64,
    /// 策略集合数量
    pub policy_sets_count: usize,
    /// 总规则数量
    pub total_rules_count: usize,
    /// 启用的规则数量
    pub enabled_rules_count: usize,
}

/// 完整的策略引擎
pub struct CompletePolicyEngine {
    /// 策略集合
    policy_sets: Arc<RwLock<HashMap<String, PolicySet>>>,
    /// 评估缓存
    evaluation_cache: Arc<RwLock<HashMap<String, PolicySetEvaluationResult>>>,
    /// 缓存TTL
    cache_ttl: Duration,
    /// 最大缓存条目数
    max_cache_entries: usize,
    /// 统计信息
    stats: Arc<RwLock<PolicyEngineStats>>,
    /// 引擎配置
    config: PolicyEngineConfig,
}

/// 策略引擎配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEngineConfig {
    /// 是否启用缓存
    pub enable_cache: bool,
    /// 缓存TTL
    pub cache_ttl: Duration,
    /// 最大缓存条目数
    pub max_cache_entries: usize,
    /// 是否启用性能监控
    pub enable_performance_monitoring: bool,
    /// 是否启用详细日志
    pub enable_detailed_logging: bool,
    /// 最大评估时间
    pub max_evaluation_time: Duration,
}

impl Default for PolicyEngineConfig {
    fn default() -> Self {
        Self {
            enable_cache: true,
            cache_ttl: Duration::from_secs(300), // 5分钟
            max_cache_entries: 10000,
            enable_performance_monitoring: true,
            enable_detailed_logging: false,
            max_evaluation_time: Duration::from_secs(10),
        }
    }
}

impl CompletePolicyEngine {
    /// 创建新的策略引擎
    pub fn new(config: PolicyEngineConfig) -> Self {
        info!("初始化完整策略引擎");

        let engine = Self {
            policy_sets: Arc::new(RwLock::new(HashMap::new())),
            evaluation_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: config.cache_ttl,
            max_cache_entries: config.max_cache_entries,
            stats: Arc::new(RwLock::new(PolicyEngineStats::default())),
            config,
        };

        info!("策略引擎初始化完成");
        engine
    }

    /// 添加策略集合
    pub async fn add_policy_set(&self, policy_set: PolicySet) -> Result<(), ErrorInfo> {
        info!("添加策略集合: {}", policy_set.name);

        let mut policy_sets = self.policy_sets.write().await;
        let policy_set_id = policy_set.id.clone();
        policy_sets.insert(policy_set_id.clone(), policy_set);

        // 清除缓存以确保策略更新生效
        self.clear_cache().await;

        // 更新统计信息
        {
            let mut stats = self.stats.write().await;
            stats.policy_sets_count = policy_sets.len();
            stats.total_rules_count = policy_sets.values().map(|ps| ps.rules.len()).sum();
            stats.enabled_rules_count = policy_sets
                .values()
                .map(|ps| ps.rules.iter().filter(|r| r.enabled).count())
                .sum();
        }

        info!("策略集合添加完成: {}", policy_set_id);
        Ok(())
    }

    /// 移除策略集合
    pub async fn remove_policy_set(&self, policy_set_id: &str) -> Result<(), ErrorInfo> {
        info!("移除策略集合: {}", policy_set_id);

        let mut policy_sets = self.policy_sets.write().await;
        if policy_sets.remove(policy_set_id).is_some() {
            self.clear_cache().await;

            // 更新统计信息
            let mut stats = self.stats.write().await;
            stats.policy_sets_count = policy_sets.len();
            stats.total_rules_count = policy_sets.values().map(|ps| ps.rules.len()).sum();
            stats.enabled_rules_count = policy_sets
                .values()
                .map(|ps| ps.rules.iter().filter(|r| r.enabled).count())
                .sum();

            info!("策略集合移除完成: {}", policy_set_id);
            Ok(())
        } else {
            Err(ErrorInfo::new(6002, format!("策略集合不存在: {}", policy_set_id))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Warning))
        }
    }

    /// 评估策略
    pub async fn evaluate(
        &self,
        policy_set_id: &str,
        context: &PolicyContext,
    ) -> Result<PolicySetEvaluationResult, ErrorInfo> {
        let start_time = std::time::Instant::now();

        // 生成缓存键
        let cache_key = if self.config.enable_cache {
            Some(self.generate_cache_key(policy_set_id, context))
        } else {
            None
        };

        // 检查缓存
        if let Some(ref cache_key) = cache_key {
            if let Some(cached_result) = self.get_cached_result(cache_key).await {
                {
                    let mut stats = self.stats.write().await;
                    stats.cache_hits += 1;
                }
                debug!("策略评估缓存命中: {}", policy_set_id);
                return Ok(cached_result);
            }
        }

        {
            let mut stats = self.stats.write().await;
            stats.cache_misses += 1;
        }

        // 获取策略集合
        let policy_sets = self.policy_sets.read().await;
        let policy_set = policy_sets.get(policy_set_id)
            .ok_or_else(|| ErrorInfo::new(6003, format!("策略集合不存在: {}", policy_set_id))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        // 执行策略评估
        let result = policy_set.evaluate(context)?;

        // 缓存结果
        if let Some(ref cache_key) = cache_key {
            self.cache_result(cache_key.clone(), result.clone()).await;
        }

        // 更新性能统计
        let execution_time_us = start_time.elapsed().as_micros() as u64;
        {
            let mut stats = self.stats.write().await;
            stats.total_evaluations += 1;

            // 更新平均评估时间
            let total_time = stats.average_evaluation_time_us * (stats.total_evaluations - 1) + execution_time_us;
            stats.average_evaluation_time_us = total_time / stats.total_evaluations;

            // 更新最慢和最快评估时间
            if stats.slowest_evaluation_time_us < execution_time_us {
                stats.slowest_evaluation_time_us = execution_time_us;
            }
            if stats.fastest_evaluation_time_us == 0 || stats.fastest_evaluation_time_us > execution_time_us {
                stats.fastest_evaluation_time_us = execution_time_us;
            }
        }

        if self.config.enable_detailed_logging {
            info!("策略评估完成: {} -> {:?} (耗时: {}μs)",
                  policy_set_id, result.final_action, execution_time_us);
        }

        Ok(result)
    }

    /// 批量评估策略
    pub async fn evaluate_multiple(
        &self,
        evaluations: Vec<(String, PolicyContext)>,
    ) -> Result<Vec<(String, Result<PolicySetEvaluationResult, ErrorInfo>)>, ErrorInfo> {
        let start_time = std::time::Instant::now();
        let mut results = Vec::new();

        info!("开始批量策略评估，共 {} 个请求", evaluations.len());

        // 并发评估
        let mut handles = Vec::new();
        for (policy_set_id, context) in evaluations {
            let policy_sets = self.policy_sets.clone();
            let evaluation_cache = self.evaluation_cache.clone();
            let cache_ttl = self.cache_ttl;
            let max_cache_entries = self.max_cache_entries;
            let stats = self.stats.clone();
            let config = self.config.clone();

            let handle = tokio::spawn(async move {
                // 创建临时的引擎实例用于评估
                let temp_engine = CompletePolicyEngine {
                    policy_sets,
                    evaluation_cache,
                    cache_ttl,
                    max_cache_entries,
                    stats,
                    config,
                };
                let result = temp_engine.evaluate(&policy_set_id, &context).await;
                (policy_set_id, result)
            });
            handles.push(handle);
        }

        // 等待所有评估完成
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => error!("批量评估任务失败: {:?}", e),
            }
        }

        let total_time = start_time.elapsed();
        info!("批量策略评估完成，耗时: {:?}", total_time);

        Ok(results)
    }

    /// 生成缓存键
    fn generate_cache_key(&self, policy_set_id: &str, context: &PolicyContext) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        policy_set_id.hash(&mut hasher);

        // 序列化上下文数据用于哈希计算
        if let Ok(context_json) = serde_json::to_string(context) {
            context_json.hash(&mut hasher);
        }

        format!("policy_eval_{:x}", hasher.finish())
    }

    /// 获取缓存结果
    async fn get_cached_result(&self, cache_key: &str) -> Option<PolicySetEvaluationResult> {
        let cache = self.evaluation_cache.read().await;
        cache.get(cache_key).cloned()
    }

    /// 缓存结果
    async fn cache_result(&self, cache_key: String, result: PolicySetEvaluationResult) {
        let mut cache = self.evaluation_cache.write().await;

        // 检查缓存大小限制
        if cache.len() >= self.max_cache_entries {
            // 简单的LRU策略：删除最旧的条目
            if let Some(oldest_key) = cache.keys().next().cloned() {
                cache.remove(&oldest_key);
            }
        }

        cache.insert(cache_key, result);
    }

    /// 清除缓存
    async fn clear_cache(&self) {
        let mut cache = self.evaluation_cache.write().await;
        cache.clear();
        debug!("策略评估缓存已清除");
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> PolicyEngineStats {
        self.stats.read().await.clone()
    }

    /// 重置统计信息
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        let policy_sets_count = stats.policy_sets_count;
        let total_rules_count = stats.total_rules_count;
        let enabled_rules_count = stats.enabled_rules_count;

        *stats = PolicyEngineStats::default();
        stats.policy_sets_count = policy_sets_count;
        stats.total_rules_count = total_rules_count;
        stats.enabled_rules_count = enabled_rules_count;

        info!("策略引擎统计信息已重置");
    }

    /// 获取所有策略集合
    pub async fn list_policy_sets(&self) -> Vec<PolicySet> {
        self.policy_sets.read().await.values().cloned().collect()
    }

    /// 根据标签查找策略集合
    pub async fn find_policy_sets_by_tag(&self, tag: &str) -> Vec<PolicySet> {
        self.policy_sets.read().await
            .values()
            .filter(|ps| ps.tags.contains(tag))
            .cloned()
            .collect()
    }

    /// 启用/禁用策略集合
    pub async fn set_policy_set_enabled(&self, policy_set_id: &str, enabled: bool) -> Result<(), ErrorInfo> {
        let mut policy_sets = self.policy_sets.write().await;
        if let Some(policy_set) = policy_sets.get_mut(policy_set_id) {
            policy_set.enabled = enabled;
            self.clear_cache().await;
            info!("策略集合 {} 已{}", policy_set_id, if enabled { "启用" } else { "禁用" });
            Ok(())
        } else {
            Err(ErrorInfo::new(6004, format!("策略集合不存在: {}", policy_set_id))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Warning))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    use tokio::time::Duration as TokioDuration;

    static INIT: Once = Once::new();

    fn init_logging() {
        INIT.call_once(|| {
            // 尝试初始化日志，如果已经初始化则忽略错误
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .try_init();
        });
    }

    fn create_test_context() -> PolicyContext {
        PolicyContext::new()
            .with_requester_id("user-123".to_string())
            .with_resource("/api/data".to_string())
            .with_operation("read".to_string())
            .set_field("ip_address".to_string(), serde_json::Value::String("192.168.1.100".to_string()))
            .set_field("role".to_string(), serde_json::Value::String("admin".to_string()))
            .set_field("department".to_string(), serde_json::Value::String("engineering".to_string()))
    }

    #[tokio::test]
    async fn test_policy_condition_evaluation() {
        init_logging();

        let context = create_test_context();

        // 测试等于条件
        let condition = PolicyCondition::new(
            "role".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("admin".to_string()),
            "用户角色检查".to_string(),
        );

        let result = condition.evaluate(&context).unwrap();
        assert!(result, "角色条件应该匹配");

        // 测试不等于条件
        let condition = PolicyCondition::new(
            "role".to_string(),
            ConditionOperator::NotEquals,
            serde_json::Value::String("guest".to_string()),
            "用户角色检查".to_string(),
        );

        let result = condition.evaluate(&context).unwrap();
        assert!(result, "角色不等于条件应该匹配");

        // 测试包含条件
        let condition = PolicyCondition::new(
            "ip_address".to_string(),
            ConditionOperator::Contains,
            serde_json::Value::String("192.168".to_string()),
            "IP地址检查".to_string(),
        );

        let result = condition.evaluate(&context).unwrap();
        assert!(result, "IP地址包含条件应该匹配");
    }

    #[tokio::test]
    async fn test_policy_rule_evaluation() {
        init_logging();

        let context = create_test_context();

        let rule = PolicyRule::new(
            "rule-1".to_string(),
            "管理员访问规则".to_string(),
            "允许管理员访问".to_string(),
            100,
            PolicyAction::Allow,
        )
        .add_condition(PolicyCondition::new(
            "role".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("admin".to_string()),
            "角色检查".to_string(),
        ))
        .add_condition(PolicyCondition::new(
            "department".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("engineering".to_string()),
            "部门检查".to_string(),
        ))
        .with_condition_combination(ConditionOperator::And);

        let result = rule.evaluate(&context).unwrap();
        assert!(result.matched, "规则应该匹配");
        assert_eq!(result.action, PolicyAction::Allow);
    }

    #[tokio::test]
    async fn test_policy_set_evaluation() {
        init_logging();

        let context = create_test_context();

        let policy_set = PolicySet::new(
            "policy-1".to_string(),
            "访问控制策略".to_string(),
            "控制用户访问权限".to_string(),
            PolicyAction::Deny,
        )
        .add_rule(PolicyRule::new(
            "allow-admin".to_string(),
            "允许管理员".to_string(),
            "管理员可以访问".to_string(),
            100,
            PolicyAction::Allow,
        )
        .add_condition(PolicyCondition::new(
            "role".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("admin".to_string()),
            "角色检查".to_string(),
        )))
        .add_rule(PolicyRule::new(
            "deny-guest".to_string(),
            "拒绝访客".to_string(),
            "访客不能访问".to_string(),
            50,
            PolicyAction::Deny,
        )
        .add_condition(PolicyCondition::new(
            "role".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("guest".to_string()),
            "角色检查".to_string(),
        )));

        let result = policy_set.evaluate(&context).unwrap();
        assert_eq!(result.final_action, PolicyAction::Allow);
        assert_eq!(result.matched_rules.len(), 1);
    }

    #[tokio::test]
    async fn test_policy_engine_basic_operations() {
        init_logging();

        let config = PolicyEngineConfig::default();
        let engine = CompletePolicyEngine::new(config);

        let policy_set = PolicySet::new(
            "test-policy".to_string(),
            "测试策略".to_string(),
            "用于测试的策略".to_string(),
            PolicyAction::Deny,
        );

        engine.add_policy_set(policy_set).await.unwrap();

        let context = create_test_context();
        let result = engine.evaluate("test-policy", &context).await.unwrap();

        assert_eq!(result.final_action, PolicyAction::Deny); // 默认动作
    }

    #[tokio::test]
    async fn test_policy_caching() {
        init_logging();

        let config = PolicyEngineConfig {
            enable_cache: true,
            cache_ttl: Duration::from_secs(60),
            ..Default::default()
        };
        let engine = CompletePolicyEngine::new(config);

        let policy_set = PolicySet::new(
            "cache-test".to_string(),
            "缓存测试策略".to_string(),
            "测试缓存功能".to_string(),
            PolicyAction::Allow,
        );

        engine.add_policy_set(policy_set).await.unwrap();

        let context = create_test_context();

        // 第一次评估
        let start = std::time::Instant::now();
        let result1 = engine.evaluate("cache-test", &context).await.unwrap();
        let first_time = start.elapsed();

        // 第二次评估（应该使用缓存）
        let start = std::time::Instant::now();
        let result2 = engine.evaluate("cache-test", &context).await.unwrap();
        let second_time = start.elapsed();

        assert_eq!(result1.final_action, result2.final_action);
        assert!(second_time < first_time, "缓存评估应该更快");

        let stats = engine.get_stats().await;
        assert!(stats.cache_hits > 0, "应该有缓存命中");
    }

    #[tokio::test]
    async fn test_batch_evaluation() {
        init_logging();

        let config = PolicyEngineConfig::default();
        let engine = CompletePolicyEngine::new(config);

        let policy_set = PolicySet::new(
            "batch-test".to_string(),
            "批量测试策略".to_string(),
            "测试批量评估".to_string(),
            PolicyAction::Allow,
        );

        engine.add_policy_set(policy_set).await.unwrap();

        let evaluations = vec![
            ("batch-test".to_string(), create_test_context()),
            ("batch-test".to_string(), create_test_context()),
            ("batch-test".to_string(), create_test_context()),
        ];

        let results = engine.evaluate_multiple(evaluations).await.unwrap();
        assert_eq!(results.len(), 3);

        for (policy_set_id, result) in results {
            assert_eq!(policy_set_id, "batch-test");
            assert!(result.is_ok());
            assert_eq!(result.unwrap().final_action, PolicyAction::Allow);
        }
    }

    #[tokio::test]
    async fn test_policy_engine_performance() {
        init_logging();

        let config = PolicyEngineConfig {
            enable_cache: true,
            enable_performance_monitoring: true,
            ..Default::default()
        };
        let engine = CompletePolicyEngine::new(config);

        // 创建复杂的策略集合
        let policy_set = PolicySet::new(
            "perf-test".to_string(),
            "性能测试策略".to_string(),
            "测试性能".to_string(),
            PolicyAction::Deny,
        )
        .add_rule(PolicyRule::new(
            "complex-rule".to_string(),
            "复杂规则".to_string(),
            "包含多个条件".to_string(),
            100,
            PolicyAction::Allow,
        )
        .add_condition(PolicyCondition::new(
            "role".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("admin".to_string()),
            "角色检查".to_string(),
        ))
        .add_condition(PolicyCondition::new(
            "department".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("engineering".to_string()),
            "部门检查".to_string(),
        ))
        .add_condition(PolicyCondition::new(
            "ip_address".to_string(),
            ConditionOperator::Contains,
            serde_json::Value::String("192.168".to_string()),
            "IP检查".to_string(),
        )));

        engine.add_policy_set(policy_set).await.unwrap();

        let context = create_test_context();

        // 性能测试
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let _ = engine.evaluate("perf-test", &context).await.unwrap();
        }
        let total_time = start.elapsed();

        assert!(total_time < TokioDuration::from_millis(1000),
                "100次评估应该在1秒内完成，实际耗时: {:?}", total_time);

        let stats = engine.get_stats().await;
        assert_eq!(stats.total_evaluations, 100);
        assert!(stats.average_evaluation_time_us > 0);
        assert!(stats.slowest_evaluation_time_us >= stats.fastest_evaluation_time_us);

        info!("性能测试完成: 100次评估耗时 {:?}", total_time);
        info!("平均评估时间: {}μs", stats.average_evaluation_time_us);
        info!("最慢评估时间: {}μs", stats.slowest_evaluation_time_us);
        info!("最快评估时间: {}μs", stats.fastest_evaluation_time_us);
    }

    #[tokio::test]
    async fn test_complex_policy_conditions() {
        init_logging();

        let context = PolicyContext::new()
            .set_field("age".to_string(), serde_json::Value::Number(serde_json::Number::from(25)))
            .set_field("score".to_string(), serde_json::Value::Number(serde_json::Number::from(85)))
            .set_field("permissions".to_string(), serde_json::Value::Array(vec![
                serde_json::Value::String("read".to_string()),
                serde_json::Value::String("write".to_string()),
            ]));

        // 测试数值比较
        let condition = PolicyCondition::new(
            "age".to_string(),
            ConditionOperator::GreaterThanOrEqual,
            serde_json::Value::Number(serde_json::Number::from(18)),
            "年龄检查".to_string(),
        );

        let result = condition.evaluate(&context).unwrap();
        assert!(result, "年龄条件应该匹配");

        // 测试数组包含
        let condition = PolicyCondition::new(
            "permissions".to_string(),
            ConditionOperator::In,
            serde_json::Value::String("read".to_string()),
            "权限检查".to_string(),
        );

        let result = condition.evaluate(&context).unwrap();
        assert!(result, "权限条件应该匹配");

        // 测试正则表达式
        let context = PolicyContext::new()
            .set_field("email".to_string(), serde_json::Value::String("user@example.com".to_string()));

        let condition = PolicyCondition::new(
            "email".to_string(),
            ConditionOperator::Regex,
            serde_json::Value::String(r".*@example\.com$".to_string()),
            "邮箱格式检查".to_string(),
        );

        let result = condition.evaluate(&context).unwrap();
        assert!(result, "邮箱正则条件应该匹配");
    }
}