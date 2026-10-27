use crate::{CsvMapping, OnError, TaskId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskContract {
    pub id: TaskId,
    pub revision: u32,
    pub goal: String,
    /// Older contracts describe CSV mappings and default to CSV.
    #[serde(default)]
    pub input_kind: InputKind,
    /// Required CSV header names, independent of order. Empty for text inputs.
    pub input_schema: Vec<String>,
    pub output_columns: Vec<String>,
    pub mapping: CsvMapping,
    pub error_policy: OnError,
    pub acceptance_checks: Vec<String>,
    pub permission_ceiling: PermissionCeiling,
    #[serde(default)]
    pub required_connections: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Text,
    #[default]
    Csv,
}

/// Symbolic ceilings. Actual selected handles and grants belong to the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PermissionCeiling {
    pub read: ReadPermission,
    pub create: CreatePermission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadPermission {
    SelectedInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatePermission {
    OneOutputInSelectedRoot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabricated_orders_contract_roundtrips() {
        let contract: TaskContract =
            serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
        assert_eq!(contract.revision, 1);
        assert_eq!(contract.input_kind, InputKind::Csv);
        assert_eq!(
            contract.input_schema,
            ["order_id", "created", "amount", "customer"]
        );
        assert_eq!(contract.output_columns, ["Order", "Date", "Amount"]);
        assert_eq!(
            contract
                .mapping
                .output_columns
                .iter()
                .map(|column| &column.name)
                .collect::<Vec<_>>(),
            contract.output_columns.iter().collect::<Vec<_>>()
        );
        assert!(contract.required_connections.is_empty());
        assert_eq!(
            serde_json::from_value::<TaskContract>(serde_json::to_value(&contract).unwrap())
                .unwrap(),
            contract
        );
    }

    #[test]
    fn connections_default_empty_and_permission_ceiling_is_finite() {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("required_connections");
        assert!(serde_json::from_value::<TaskContract>(value.clone())
            .unwrap()
            .required_connections
            .is_empty());
        value["permission_ceiling"]["read"] = "all_files".into();
        assert!(serde_json::from_value::<TaskContract>(value).is_err());
    }
}
