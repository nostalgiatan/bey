//! # 策略规则模块
//!
//! 定义和评估策略规则

use error::ErrorInfo;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;
use super::types::{PolicyAction, ConditionOperator};
use super::context::PolicyContext;
use super::condition::{PolicyCondition, ConditionEvaluationResult};

/// 策略规则
///
/// 定义一个完整的策略规则，包含多个条件和执行动作
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
    ///
    /// # 参数
    ///
    /// * `id` - 规则唯一标识
    /// * `name` - 规则名称
    /// * `description` - 规则描述
    /// * `priority` - 规则优先级（数值越大优先级越高）
    /// * `action` - 规则匹配时执行的动作
    ///
    /// # 返回值
    ///
    /// 返回新创建的策略规则
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
    ///
    /// # 参数
    ///
    /// * `condition` - 要添加的条件
    ///
    /// # 返回值
    ///
    /// 返回修改后的规则（支持链式调用）
    pub fn add_condition(mut self, condition: PolicyCondition) -> Self {
        self.conditions.push(condition);
        self.updated_at = SystemTime::now();
        self
    }

    /// 设置条件组合方式
    ///
    /// # 参数
    ///
    /// * `combination` - 条件组合方式（And 或 Or）
    ///
    /// # 返回值
    ///
    /// 返回修改后的规则（支持链式调用）
    pub fn with_condition_combination(mut self, combination: ConditionOperator) -> Self {
        self.condition_combination = combination;
        self.updated_at = SystemTime::now();
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
    /// 返回修改后的规则（支持链式调用）
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tags.insert(tag);
        self
    }

    /// 添加元数据
    ///
    /// # 参数
    ///
    /// * `key` - 元数据键
    /// * `value` - 元数据值
    ///
    /// # 返回值
    ///
    /// 返回修改后的规则（支持链式调用）
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self.updated_at = SystemTime::now();
        self
    }

    /// 评估规则
    ///
    /// # 参数
    ///
    /// * `context` - 策略上下文
    ///
    /// # 返回值
    ///
    /// 返回规则评估结果或错误信息
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

/// 策略评估结果
///
/// 记录单个规则的评估结果
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
