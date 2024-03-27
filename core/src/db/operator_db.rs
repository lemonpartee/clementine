use std::ops::{Deref, DerefMut};

use clementine_circuits::PreimageType;

use crate::{
    operator::OperatorClaimSigs,
    PreimageTree,
};

use super::common_db::CommonMockDB;

#[derive(Debug, Clone)]
pub struct OperatorMockDB {
    common_db: CommonMockDB,
    deposit_take_sigs: Vec<OperatorClaimSigs>,
    connector_tree_preimages: Vec<PreimageTree>,
}

impl OperatorMockDB {
    pub fn new() -> Self {
        Self {
            common_db: CommonMockDB::new(),
            deposit_take_sigs: Vec::new(),
            connector_tree_preimages: Vec::new(),
        }
    }
}


impl Deref for OperatorMockDB {
    type Target = CommonMockDB;

    fn deref(&self) -> &Self::Target {
        &self.common_db
    }
}

impl DerefMut for OperatorMockDB {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.common_db
    }
}

impl OperatorMockDB {
    pub fn get_deposit_index(&self) -> usize {
        self.deposit_take_sigs.len()
    }

    // fn get_deposit_take_sigs(&self) -> Vec<OperatorClaimSigs> {
    //     self.deposit_take_sigs.clone()
    // }

    pub fn add_deposit_take_sigs(&mut self, deposit_take_sigs: OperatorClaimSigs) {
        self.deposit_take_sigs.push(deposit_take_sigs);
    }

    pub fn get_connector_tree_preimages_level(&self, period: usize, level: usize) -> Vec<PreimageType> {
        self.connector_tree_preimages[period][level].clone()
    }

    pub fn get_connector_tree_preimages(
        &self,
        period: usize,
        level: usize,
        idx: usize,
    ) -> PreimageType {
        self.connector_tree_preimages[period][level][idx].clone()
    }

    pub fn set_connector_tree_preimages(
        &mut self,
        connector_tree_preimages: Vec<Vec<Vec<PreimageType>>>,
    ) {
        self.connector_tree_preimages = connector_tree_preimages;
    }

}
