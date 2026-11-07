//! MCP boundary parsing: unique keys and the frozen enum representations.
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

/// The MCP enum contracts use strings, never serde's object-tag alternative.
pub(super) fn string_enum<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(de::value::StringDeserializer::<D::Error>::new(
        String::deserialize(deserializer)?,
    ))
}

pub(super) fn catalog_filter<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<enouia_memory_contract::next::CatalogFilter, D::Error> {
    let value = Value::deserialize(deserializer)?;
    for field in [
        "form",
        "state",
        "knowledge",
        "adoption",
        "validity",
        "attention",
        "time_axis",
    ] {
        if value
            .get(field)
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(de::Error::custom(
                "catalog filter enum requires string or null",
            ));
        }
    }
    enouia_memory_contract::next::CatalogFilter::deserialize(value).map_err(de::Error::custom)
}

/// Required, nullable search cursor whose selection algorithm is a string.
pub(super) fn catalog_cursor<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<enouia_memory_contract::next::CatalogCursor>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    if value["selection"]
        .get("algorithm")
        .is_some_and(|v| !v.is_string())
    {
        return Err(de::Error::custom(
            "catalog cursor algorithm requires string",
        ));
    }
    Option::deserialize(value).map_err(de::Error::custom)
}

struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Json;
        impl<'de> Visitor<'de> for Json {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate object fields")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("invalid number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(Value::String(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Unique, A::Error> {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom("duplicate field"));
                    }
                    let Unique(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(Unique(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Json)
    }
}
pub(crate) fn parse(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    serde_json::from_slice::<Unique>(bytes).map(|v| v.0)
}
