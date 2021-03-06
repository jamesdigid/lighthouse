extern crate db;
extern crate naive_fork_choice;
extern crate ssz;
extern crate ssz_helpers;
extern crate state_transition;
extern crate types;
extern crate validation;
extern crate validator_induction;
extern crate validator_shuffling;

mod block_context;
mod block_processing;
mod genesis;
mod maps;
mod stores;
mod transition;

use db::ClientDB;
use genesis::genesis_states;
use maps::{generate_attester_and_proposer_maps, AttesterAndProposerMapError};
use std::collections::HashMap;
use std::sync::Arc;
use stores::BeaconChainStore;
use types::{ActiveState, AttesterMap, ChainConfig, CrystallizedState, Hash256, ProposerMap};

#[derive(Debug, PartialEq)]
pub enum BeaconChainError {
    InvalidGenesis,
    InsufficientValidators,
    UnableToGenerateMaps(AttesterAndProposerMapError),
    DBError(String),
}

pub struct BeaconChain<T: ClientDB + Sized> {
    /// The last slot which has been finalized, this is common to all forks.
    pub last_finalized_slot: u64,
    /// A vec of all block heads (tips of chains).
    pub head_block_hashes: Vec<Hash256>,
    /// The index of the canonical block in `head_block_hashes`.
    pub canonical_head_block_hash: usize,
    /// A map where the value is an active state the the key is its hash.
    pub active_states: HashMap<Hash256, ActiveState>,
    /// A map where the value is crystallized state the the key is its hash.
    pub crystallized_states: HashMap<Hash256, CrystallizedState>,
    /// A map of crystallized state to a proposer and attester map.
    pub attester_proposer_maps: HashMap<Hash256, (Arc<AttesterMap>, Arc<ProposerMap>)>,
    /// A collection of database stores used by the chain.
    pub store: BeaconChainStore<T>,
    /// The chain configuration.
    pub config: ChainConfig,
}

impl<T> BeaconChain<T>
where
    T: ClientDB + Sized,
{
    pub fn new(store: BeaconChainStore<T>, config: ChainConfig) -> Result<Self, BeaconChainError> {
        if config.initial_validators.is_empty() {
            return Err(BeaconChainError::InsufficientValidators);
        }

        let (active_state, crystallized_state) = genesis_states(&config)?;

        let canonical_latest_block_hash = Hash256::zero();
        let head_block_hashes = vec![canonical_latest_block_hash];
        let canonical_head_block_hash = 0;
        let mut active_states = HashMap::new();
        let mut crystallized_states = HashMap::new();
        let mut attester_proposer_maps = HashMap::new();

        let (attester_map, proposer_map) = generate_attester_and_proposer_maps(
            &crystallized_state.shard_and_committee_for_slots,
            0,
        )?;

        active_states.insert(canonical_latest_block_hash, active_state);
        crystallized_states.insert(canonical_latest_block_hash, crystallized_state);
        attester_proposer_maps.insert(
            canonical_latest_block_hash,
            (Arc::new(attester_map), Arc::new(proposer_map)),
        );

        Ok(Self {
            last_finalized_slot: 0,
            head_block_hashes,
            canonical_head_block_hash,
            active_states,
            crystallized_states,
            attester_proposer_maps,
            store,
            config,
        })
    }

    pub fn canonical_block_hash(&self) -> Hash256 {
        self.head_block_hashes[self.canonical_head_block_hash]
    }
}

impl From<AttesterAndProposerMapError> for BeaconChainError {
    fn from(e: AttesterAndProposerMapError) -> BeaconChainError {
        BeaconChainError::UnableToGenerateMaps(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use db::stores::*;
    use db::MemoryDB;
    use std::sync::Arc;
    use types::ValidatorRegistration;

    #[test]
    fn test_new_chain() {
        let mut config = ChainConfig::standard();
        config.cycle_length = 4;
        config.shard_count = 4;
        let db = Arc::new(MemoryDB::open());
        let store = BeaconChainStore {
            block: Arc::new(BeaconBlockStore::new(db.clone())),
            pow_chain: Arc::new(PoWChainStore::new(db.clone())),
            validator: Arc::new(ValidatorStore::new(db.clone())),
        };

        for _ in 0..config.cycle_length * 2 {
            config
                .initial_validators
                .push(ValidatorRegistration::random())
        }

        let chain = BeaconChain::new(store, config.clone()).unwrap();
        let (act, cry) = genesis_states(&config).unwrap();

        assert_eq!(chain.last_finalized_slot, 0);
        assert_eq!(chain.canonical_block_hash(), Hash256::zero());

        let stored_act = chain.active_states.get(&Hash256::zero()).unwrap();
        assert_eq!(act, *stored_act);

        let stored_cry = chain.crystallized_states.get(&Hash256::zero()).unwrap();
        assert_eq!(cry, *stored_cry);
    }
}
