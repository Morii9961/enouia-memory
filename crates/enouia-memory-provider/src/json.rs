//! Parse provider JSON without silently overwriting duplicate object members.
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use std::fmt;

pub(crate) fn parse(bytes: &[u8]) -> serde_json::Result<Value> {
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueValue::deserialize(&mut decoder)?.0;
    decoder.end()?;
    Ok(value)
}

struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        decoder.deserialize_any(UniqueVisitor)
    }
}
struct UniqueVisitor;
impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = UniqueValue;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON with unique object members")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Bool(value)))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(value.into())))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(value.into())))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .map(UniqueValue)
            .ok_or_else(|| E::custom("nonfinite provider JSON number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(value.into())))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(value)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<UniqueValue>()? {
            values.push(value.0);
        }
        Ok(UniqueValue(Value::Array(values)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Self::Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate provider JSON member"));
            }
            values.insert(key, object.next_value::<UniqueValue>()?.0);
        }
        Ok(UniqueValue(Value::Object(values)))
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::Value;
    #[test]
    fn unique_json_preserves_values_and_refuses_nested_escaped_duplicate_members() {
        for input in [
            r#"{"values":[null,true,false,-3,18446744073709551615,0.7,1.5e1],"text":"中文\n\"quoted\""}"#,
            r#"{"a":1,"A":2,"ａ":3,"nested":[{"a":1},{"a":2}]}"#,
            r#"{"payload":"{\"usage\":1,\"usage\":2}"}"#,
        ] {
            assert_eq!(
                parse(input.as_bytes()).unwrap(),
                serde_json::from_str::<Value>(input).unwrap()
            );
        }
        for input in [
            r#"{"usage":1,"usage":2}"#,
            r#"{"usage":1,"\u0075sage":2}"#,
            r#"{"metadata":{"ignored":1,"ignored":2}}"#,
            r#"{"output":[{"type":"message","type":"reasoning"}]}"#,
            r#"{"usage":{"input_tokens":50000,"input_tokens":100}}"#,
        ] {
            assert!(parse(input.as_bytes()).is_err(), "{input}");
        }
    }
    #[test]
    fn unique_json_keeps_single_value_and_recursion_limits() {
        assert_eq!(
            parse(b" \n {\"number\":-0.5e2} \t").unwrap(),
            serde_json::json!({"number":-50.0})
        );
        for input in [b"{}{}".as_slice(), b"{} trailing", b"1e999", b"{\"a\":}"] {
            assert!(parse(input).is_err());
        }
        let deep = format!("{}null{}", "[".repeat(200), "]".repeat(200));
        assert!(parse(deep.as_bytes()).is_err());
    }
}
