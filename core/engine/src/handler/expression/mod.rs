use crate::handler::node::{NodeRequest, NodeResponse, NodeResult};
use crate::model::DecisionNodeKind;
use std::collections::HashMap;

use crate::util::json_map::FlatJsonMap;
use anyhow::{anyhow, Context};
use serde::Serialize;
use serde_json::Value;
use zen_expression::variable::ToVariable;
use zen_expression::Isolate;

pub struct ExpressionHandler<'a> {
    trace: bool,
    isolate: Isolate<'a>,
}

#[derive(Debug, Serialize)]
struct ExpressionTrace {
    result: String,
}

impl<'a> ExpressionHandler<'a> {
    pub fn new(trace: bool) -> Self {
        Self {
            trace,
            isolate: Isolate::new(),
        }
    }

    pub async fn handle(&mut self, request: &'a NodeRequest<'_>) -> NodeResult {
        let content = match &request.node.kind {
            DecisionNodeKind::ExpressionNode { content } => Ok(content),
            _ => Err(anyhow!("Unexpected node type")),
        }?;

        let mut result = FlatJsonMap::with_capacity(content.expressions.len());
        let mut trace_map = self.trace.then(|| HashMap::<&str, ExpressionTrace>::new());

        self.isolate.set_environment(&request.input);
        for expression in &content.expressions {
            let value = self.evaluate_expression(&expression.value)?;
            if let Some(tmap) = &mut trace_map {
                tmap.insert(
                    &expression.key,
                    ExpressionTrace {
                        result: serde_json::to_string(&value).unwrap_or("Error".to_owned()),
                    },
                );
            }

            self.isolate.update_environment(|arena, env| {
                let Some(environment) = env else {
                    return;
                };

                let key = format!("$.{}", &expression.key);
                let _ =
                    environment.dot_insert(arena, key.as_str(), value.to_variable(arena).unwrap());
            });
            result.insert(&expression.key, value);
        }

        let output = result.to_json().context("Conversion to JSON failed")?;
        Ok(NodeResponse {
            output,
            trace_data: trace_map
                .map(|tm| serde_json::to_value(tm))
                .transpose()
                .context("Failed to serialize trace data")?,
        })
    }

    fn evaluate_expression(&mut self, expression: &'a str) -> anyhow::Result<Value> {
        self.isolate
            .run_standard(expression)
            .with_context(|| format!(r#"Failed to evaluate expression: "{expression}""#))
    }
}
