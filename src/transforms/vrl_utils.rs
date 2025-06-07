use serde_json;
use std::collections::BTreeMap;
use vrl::prelude::NotNan;
use vrl::value::Value as VrlValue;

pub fn convert_to_vrl_value(value: &serde_json::Value) -> VrlValue {
    match value {
        serde_json::Value::Null => VrlValue::Null,
        serde_json::Value::Bool(b) => VrlValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                VrlValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                VrlValue::Float(NotNan::new(f).unwrap())
            } else {
                VrlValue::Null
            }
        }
        serde_json::Value::String(s) => VrlValue::Bytes(s.clone().into()),
        serde_json::Value::Array(a) => {
            VrlValue::Array(a.iter().map(convert_to_vrl_value).collect())
        }
        serde_json::Value::Object(o) => {
            let mut map = BTreeMap::new();
            for (k, v) in o {
                map.insert(k.clone().into(), convert_to_vrl_value(v));
            }
            VrlValue::Object(map)
        }
    }
}

pub fn convert_from_vrl_value(value: VrlValue) -> serde_json::Value {
    match value {
        VrlValue::Null => serde_json::Value::Null,
        VrlValue::Boolean(b) => serde_json::Value::Bool(b),
        VrlValue::Integer(i) => serde_json::Value::Number(i.into()),
        VrlValue::Float(f) => {
            if let Some(num) = serde_json::Number::from_f64(f.into()) {
                serde_json::Value::Number(num)
            } else {
                serde_json::Value::Null
            }
        }
        VrlValue::Bytes(b) => serde_json::Value::String(String::from_utf8_lossy(&b).into_owned()),
        VrlValue::Array(a) => {
            serde_json::Value::Array(a.into_iter().map(convert_from_vrl_value).collect())
        }
        VrlValue::Timestamp(t) => serde_json::Value::Number(t.timestamp().into()),
        VrlValue::Object(o) => {
            let mut map = serde_json::Map::new();
            for (k, v) in o {
                map.insert(k.to_string(), convert_from_vrl_value(v));
            }
            serde_json::Value::Object(map)
        }
        _ => serde_json::Value::Null,
    }
}
