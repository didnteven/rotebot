use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Finite host-owned operations only: no expressions, scripts, or extension payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CsvMapping {
    pub output_columns: Vec<OutputColumn>,
    pub on_error: OnError,
    #[serde(default)]
    pub skip_header_rows: u32,
    #[serde(default = "formula_escape_default")]
    pub formula_escape: bool,
}

fn formula_escape_default() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputColumn {
    pub name: String,
    /// Input column name; ignored by the Constant operation.
    pub source: String,
    pub transform: Transform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Transform {
    Copy,
    Trim,
    Upper,
    Lower,
    Date {
        from: DateFormat,
        to: DateFormat,
    },
    /// Decimal strings only. Execution must never parse money through f64.
    Money {
        currency: String,
        decimals: u8,
    },
    Constant {
        value: String,
    },
}

impl<'de> Deserialize<'de> for Transform {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde ignores extra fields on internally tagged unit variants, even
        // with deny_unknown_fields. Empty struct variants enforce the finite
        // wire shape while preserving ergonomic public unit variants.
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum StrictTransform {
            Copy {},
            Trim {},
            Upper {},
            Lower {},
            Date { from: DateFormat, to: DateFormat },
            Money { currency: String, decimals: u8 },
            Constant { value: String },
        }
        Ok(match StrictTransform::deserialize(deserializer)? {
            StrictTransform::Copy {} => Self::Copy,
            StrictTransform::Trim {} => Self::Trim,
            StrictTransform::Upper {} => Self::Upper,
            StrictTransform::Lower {} => Self::Lower,
            StrictTransform::Date { from, to } => Self::Date { from, to },
            StrictTransform::Money { currency, decimals } => Self::Money { currency, decimals },
            StrictTransform::Constant { value } => Self::Constant { value },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    MmDdYy,
    MmDdYyyy,
    DdMmYy,
    DdMmYyyy,
    YyyyMmDd,
    Iso8601,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    StopAndExplain,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn safe_defaults_and_roundtrip() {
        let value = json!({"output_columns": [], "on_error": "stop_and_explain"});
        let mapping: CsvMapping = serde_json::from_value(value).unwrap();
        assert_eq!(mapping.skip_header_rows, 0);
        assert!(mapping.formula_escape);
        assert_eq!(
            serde_json::from_value::<CsvMapping>(serde_json::to_value(&mapping).unwrap()).unwrap(),
            mapping
        );
    }

    #[test]
    fn every_finite_transform_roundtrips() {
        let transforms = [
            Transform::Copy,
            Transform::Trim,
            Transform::Upper,
            Transform::Lower,
            Transform::Date {
                from: DateFormat::MmDdYy,
                to: DateFormat::YyyyMmDd,
            },
            Transform::Money {
                currency: "USD".into(),
                decimals: 2,
            },
            Transform::Constant {
                value: "9007199254740993.01".into(),
            },
        ];
        for transform in transforms {
            assert_eq!(
                serde_json::from_value::<Transform>(serde_json::to_value(&transform).unwrap())
                    .unwrap(),
                transform
            );
        }
        for format in [
            DateFormat::MmDdYy,
            DateFormat::MmDdYyyy,
            DateFormat::DdMmYy,
            DateFormat::DdMmYyyy,
            DateFormat::YyyyMmDd,
            DateFormat::Iso8601,
        ] {
            assert_eq!(
                serde_json::from_value::<DateFormat>(serde_json::to_value(format).unwrap())
                    .unwrap(),
                format
            );
        }
    }

    #[test]
    fn rejects_scripts_expressions_and_unknown_operations() {
        for value in [
            json!({"type":"script", "code":"return 1"}),
            json!({"type":"copy", "expression":"input.amount"}),
            json!({"type":"money", "currency":"USD", "decimals":256}),
        ] {
            assert!(serde_json::from_value::<Transform>(value).is_err());
        }
        assert!(serde_json::from_value::<CsvMapping>(json!({
            "output_columns":[], "on_error":"stop_and_explain", "script":"alert(1)"
        }))
        .is_err());
    }
}
