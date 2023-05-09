use std::time::SystemTime;

use tracing_actions::{ActionEvent, AttributeValue, SpanStatus, TraceKind};

use crate::{
    proto::opentelemetry::{
        common::v1::{any_value, AnyValue, KeyValue},
        trace::v1::{
            span::{self, Event},
            status::StatusCode,
            Span, Status,
        },
    },
    OtlpAttributes,
};

impl From<OtlpAttributes> for Vec<KeyValue> {
    fn from(value: OtlpAttributes) -> Self {
        let mut attributes: Vec<KeyValue> = value
            .other_attributes
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| KeyValue {
                key,
                value: Some(AnyValue {
                    value: Some(any_value::Value::StringValue(value)),
                }),
            })
            .collect();
        attributes.push(KeyValue {
            key: "service.name".to_string(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue(value.service_name)),
            }),
        });
        attributes
    }
}

impl From<&mut tracing_actions::ActionSpan> for Span {
    fn from(value: &mut tracing_actions::ActionSpan) -> Self {
        Self {
            trace_id: value.trace_id.to_vec(),
            span_id: value.span_id.to_vec(),
            trace_state: value.trace_state.clone(),
            parent_span_id: value
                .parent_span_id
                .map(|id| id.to_vec())
                .unwrap_or_default(),
            name: value
                .metadata
                .map(|m| m.name().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            kind: as_spankind(value.kind) as i32,
            start_time_unix_nano: value
                .start
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            end_time_unix_nano: value
                .end
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            attributes: value.attributes.drain().map(KeyValue::from).collect(),
            dropped_attributes_count: 0,
            events: value.events.drain(..).map(Event::from).collect(),
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: Some(value.status.into()),
        }
    }
}

fn as_spankind(value: TraceKind) -> span::SpanKind {
    match value {
        TraceKind::Client => span::SpanKind::Client,
        TraceKind::Server => span::SpanKind::Server,
    }
}

impl From<(&str, AttributeValue)> for KeyValue {
    fn from(value: (&str, AttributeValue)) -> Self {
        let (name, value) = value;
        Self {
            key: name.to_string(),
            value: Some(AnyValue {
                value: Some(match value {
                    AttributeValue::String(s) => any_value::Value::StringValue(s),
                    AttributeValue::F64(f) => any_value::Value::DoubleValue(f),
                    AttributeValue::I64(i) => any_value::Value::IntValue(i),
                    AttributeValue::U64(u) => any_value::Value::IntValue(u as i64),
                    AttributeValue::I128(i) => any_value::Value::IntValue(i as i64),
                    AttributeValue::U128(u) => any_value::Value::IntValue(u as i64),
                    AttributeValue::Bool(b) => any_value::Value::BoolValue(b),
                    AttributeValue::Error(e) => any_value::Value::StringValue(e),
                }),
            }),
        }
    }
}

impl From<ActionEvent> for Event {
    fn from(mut value: ActionEvent) -> Self {
        Self {
            time_unix_nano: value
                .timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            name: value.metadata.name().to_string(),
            attributes: value.attributes.drain().map(KeyValue::from).collect(),
            dropped_attributes_count: 0,
        }
    }
}

impl From<SpanStatus> for Status {
    fn from(value: SpanStatus) -> Self {
        match value {
            SpanStatus::Unset => Self {
                message: "traces should set status".to_string(),
                code: StatusCode::Unset.into(),
            },
            SpanStatus::Ok => Self {
                message: "".to_string(),
                code: StatusCode::Ok.into(),
            },
            SpanStatus::Error => Self {
                message: "".to_string(),
                code: StatusCode::Error.into(),
            },
        }
    }
}
