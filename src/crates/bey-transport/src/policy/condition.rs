//! # 策略条件模块
//!
//! 定义和评估策略条件

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use tracing::debug;
use super::types::ConditionOperator;
use super::context::PolicyContext;
use crate::error_codes::policy as policy_errors;

/// 策略条件
///
/// 定义一个可评估的策略条件，包含字段、操作符和期望值
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
    ///
    /// # 参数
    ///
    /// * `field` - 要检查的字段名
    /// * `operator` - 比较操作符
    /// * `value` - 期望的值
    /// * `description` - 条件的描述信息
    ///
    /// # 返回值
    ///
    /// 返回新创建的策略条件
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
    ///
    /// # 参数
    ///
    /// * `weight` - 条件权重值
    ///
    /// # 返回值
    ///
    /// 返回修改后的条件（支持链式调用）
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// 评估条件是否满足
    ///
    /// # 参数
    ///
    /// * `context` - 策略上下文，包含用于评估的数据
    ///
    /// # 返回值
    ///
    /// 返回评估结果（true表示满足，false表示不满足）或错误信息
    pub fn evaluate(&self, context: &PolicyContext) -> Result<bool, ErrorInfo> {
        // 获取字段值，如果不存在则使用Null
        let field_value = context.get_field_value(&self.field)
            .unwrap_or(&serde_json::Value::Null);

        // 根据操作符进行评估
        let result = match (&self.operator, field_value, &self.value) {
            // 相等性判断
            (ConditionOperator::Equals, actual, expected) => actual == expected,
            (ConditionOperator::NotEquals, actual, expected) => actual != expected,
            
            // 数值比较 - 大于
            (ConditionOperator::GreaterThan, serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => {
                let actual_f64 = actual.as_f64().unwrap_or(0.0);
                let expected_f64 = expected.as_f64().unwrap_or(0.0);
                actual_f64 > expected_f64
            }
            
            // 数值比较 - 大于等于
            (ConditionOperator::GreaterThanOrEqual, serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => {
                let actual_f64 = actual.as_f64().unwrap_or(0.0);
                let expected_f64 = expected.as_f64().unwrap_or(0.0);
                actual_f64 >= expected_f64
            }
            
            // 数值比较 - 小于
            (ConditionOperator::LessThan, serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => {
                let actual_f64 = actual.as_f64().unwrap_or(0.0);
                let expected_f64 = expected.as_f64().unwrap_or(0.0);
                actual_f64 < expected_f64
            }
            
            // 数值比较 - 小于等于
            (ConditionOperator::LessThanOrEqual, serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => {
                let actual_f64 = actual.as_f64().unwrap_or(0.0);
                let expected_f64 = expected.as_f64().unwrap_or(0.0);
                actual_f64 <= expected_f64
            }
            
            // 字符串包含判断
            (ConditionOperator::Contains, serde_json::Value::String(actual), serde_json::Value::String(expected)) => {
                actual.contains(expected)
            }
            (ConditionOperator::NotContains, serde_json::Value::String(actual), serde_json::Value::String(expected)) => {
                !actual.contains(expected)
            }
            
            // 数组包含判断
            (ConditionOperator::In, actual, serde_json::Value::Array(expected)) => {
                expected.contains(actual)
            }
            (ConditionOperator::NotIn, actual, serde_json::Value::Array(expected)) => {
                !expected.contains(actual)
            }
            
            // 正则表达式匹配
            (ConditionOperator::Regex, serde_json::Value::String(actual), serde_json::Value::String(pattern)) => {
                let regex = regex::Regex::new(pattern)
                    .map_err(|e| ErrorInfo::new(policy_errors::INVALID_REGEX, format!("无效的正则表达式: {}", e))
                        .with_category(ErrorCategory::Configuration)
                        .with_severity(ErrorSeverity::Error))?;
                regex.is_match(actual)
            }
            
            // 其他情况返回false
            _ => false,
        };

        debug!("条件评估: {} {} {} = {:?}", self.field, self.operator, self.value, result);
        Ok(result)
    }
}

/// 条件评估结果
///
/// 记录单个条件的评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionEvaluationResult {
    /// 评估的条件
    pub condition: PolicyCondition,
    /// 评估结果
    pub result: bool,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
}
