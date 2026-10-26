use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4().to_string())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

string_id!(ToolId);
string_id!(BuildId);
string_id!(RunId);
string_id!(JobId);
string_id!(ConnectorId);
string_id!(TaskId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_v4_strings_and_roundtrip() {
        macro_rules! check {
            ($ty:ident) => {{
                let id = $ty::new();
                assert_ne!(id, $ty::new());
                assert_eq!(uuid::Uuid::parse_str(&id.0).unwrap().get_version_num(), 4);
                let json = serde_json::to_string(&id).unwrap();
                assert_eq!(serde_json::from_str::<$ty>(&json).unwrap(), id);
                assert_eq!(json, format!("\"{id}\""));
            }};
        }
        check!(ToolId);
        check!(BuildId);
        check!(RunId);
        check!(JobId);
        check!(ConnectorId);
        check!(TaskId);
    }
}
