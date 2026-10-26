//! Host-owned storage. `db` is the SQLCipher product database; `MemoryStore` is the
//! in-memory seam used by early shells and tests.

pub mod db;

use macroscope_domain::{TaskContract, TaskId, ToolId, ToolManifest};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("storage is unavailable")]
    Unavailable,
}

pub trait Store {
    fn put_contract(&mut self, contract: TaskContract) -> Result<(), StoreError>;
    fn contract(&self, id: &TaskId) -> Result<Option<TaskContract>, StoreError>;
    fn put_tool(&mut self, tool: ToolManifest) -> Result<(), StoreError>;
    fn list_tools(&self) -> Result<Vec<ToolManifest>, StoreError>;
}

#[derive(Default)]
pub struct MemoryStore {
    contracts: HashMap<TaskId, TaskContract>,
    tools: HashMap<ToolId, ToolManifest>,
}

impl Store for MemoryStore {
    fn put_contract(&mut self, contract: TaskContract) -> Result<(), StoreError> {
        self.contracts.insert(contract.id.clone(), contract);
        Ok(())
    }
    fn contract(&self, id: &TaskId) -> Result<Option<TaskContract>, StoreError> {
        Ok(self.contracts.get(id).cloned())
    }
    fn put_tool(&mut self, tool: ToolManifest) -> Result<(), StoreError> {
        self.tools.insert(tool.id.clone(), tool);
        Ok(())
    }
    fn list_tools(&self) -> Result<Vec<ToolManifest>, StoreError> {
        let mut tools: Vec<_> = self.tools.values().cloned().collect();
        tools.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        Ok(tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_contracts_are_isolated_snapshots() {
        let mut store = MemoryStore::default();
        let mut contract: TaskContract =
            serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
        let id = contract.id.clone();
        store.put_contract(contract.clone()).unwrap();
        contract.goal = "edited".into();
        assert_ne!(store.contract(&id).unwrap().unwrap().goal, contract.goal);
        store.put_contract(contract.clone()).unwrap();
        assert_eq!(store.contract(&id).unwrap(), Some(contract));
        assert!(store.contract(&TaskId::new()).unwrap().is_none());
        assert!(store.list_tools().unwrap().is_empty());
    }
}
