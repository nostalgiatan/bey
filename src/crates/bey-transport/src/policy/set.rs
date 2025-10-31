//! # 策略集合模块
//!
//! 定义和评估策略集合

use error::ErrorInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::SystemTime;
use super::types::PolicyAction;
use super::context::PolicyContext;
use super::rule::{PolicyRule, PolicyEvaluationResult};

/// 策略集合
///
/// 包含多个策略规则的集合，按优先级评估
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
    ///
    /// # 参数
    ///
    /// * `id` - 策略集合唯一标识
    /// * `name` - 策略集合名称
    /// * `description` - 策略集合描述
    /// * `default_action` - 默认动作（当没有规则匹配时）
    ///
    /// # 返回值
    ///
    /// 返回新创建的策略集合
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
    ///
    /// # 参数
    ///
    /// * `rule` - 要添加的规则
    ///
    /// # 返回值
    ///
    /// 返回修改后的策略集合（支持链式调用）
    pub fn add_rule(mut self, rule: PolicyRule) -> Self {
        self.rules.push(rule);
        self.updated_at = SystemTime::now();
        self
    }

    /// 根据优先级排序规则
    ///
    /// 按照规则的优先级从高到低排序
    pub fn sort_rules_by_priority(&mut self) {
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 评估策略集合
    ///
    /// # 参数
    ///
    /// * `context` - 策略上下文
    ///
    /// # 返回值
    ///
    /// 返回策略集合评估结果或错误信息
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

        // 确定最终动作 - 选择匹配规则中优先级最高的
        let matched_rules_count = matched_rules.len();
        let final_action = if let Some(highest_priority_rule) = matched_rules
            .iter()
            .max_by_key(|r| {
                self.rules
                    .iter()
                    .find(|rule| rule.id == r.rule_id)
                    .map(|rule| rule.priority)
                    .unwrap_or(0)
            })
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
///
/// 记录整个策略集合的评估结果
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
