use solana_sdk::pubkey::Pubkey;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global state management, thread-safe implementation without locks
pub struct GlobalState {
    /// Last processed slot
    last_slot: AtomicU64,
    /// Developer address array
    dev_addresses: parking_lot::RwLock<Vec<Pubkey>>,
    /// Bonk developer address array
    bonk_dev_addresses: parking_lot::RwLock<Vec<Pubkey>>,
}

impl GlobalState {
    /// Create a new global state instance
    pub fn new() -> Self {
        Self {
            last_slot: AtomicU64::new(0),
            dev_addresses: parking_lot::RwLock::new(Vec::new()),
            bonk_dev_addresses: parking_lot::RwLock::new(Vec::new()),
        }
    }

    /// Get current slot
    pub fn get_last_slot(&self) -> u64 {
        self.last_slot.load(Ordering::Relaxed)
    }

    /// Update slot, clear arrays if slot changes
    pub fn update_slot(&self, new_slot: u64) {
        let old_slot = self.last_slot.swap(new_slot, Ordering::Relaxed);

        if old_slot != new_slot {
            // Clear arrays when slot changes
            let mut dev_addresses = self.dev_addresses.write();
            let mut bonk_dev_addresses = self.bonk_dev_addresses.write();

            dev_addresses.clear();
            bonk_dev_addresses.clear();
        }
    }

    /// Add developer address
    pub fn add_dev_address(&self, address: Pubkey) {
        let mut dev_addresses = self.dev_addresses.write();
        if !dev_addresses.contains(&address) {
            dev_addresses.push(address);
        }
    }

    /// Check if address is a developer address
    pub fn is_dev_address(&self, address: &Pubkey) -> bool {
        let dev_addresses = self.dev_addresses.read();
        dev_addresses.contains(address)
    }

    /// Add Bonk developer address
    pub fn add_bonk_dev_address(&self, address: Pubkey) {
        let mut bonk_dev_addresses = self.bonk_dev_addresses.write();
        if !bonk_dev_addresses.contains(&address) {
            bonk_dev_addresses.push(address);
        }
    }

    /// Check if address is a Bonk developer address
    pub fn is_bonk_dev_address(&self, address: &Pubkey) -> bool {
        let bonk_dev_addresses = self.bonk_dev_addresses.read();
        bonk_dev_addresses.contains(address)
    }

    /// Get all developer addresses
    pub fn get_dev_addresses(&self) -> Vec<Pubkey> {
        let dev_addresses = self.dev_addresses.read();
        dev_addresses.clone()
    }

    /// Get all Bonk developer addresses
    pub fn get_bonk_dev_addresses(&self) -> Vec<Pubkey> {
        let bonk_dev_addresses = self.bonk_dev_addresses.read();
        bonk_dev_addresses.clone()
    }

    /// Clear all data
    pub fn clear_all_data(&self) {
        let mut dev_addresses = self.dev_addresses.write();
        let mut bonk_dev_addresses = self.bonk_dev_addresses.write();

        dev_addresses.clear();
        bonk_dev_addresses.clear();
    }
}

impl Default for GlobalState {
    fn default() -> Self {
        Self::new()
    }
}

/// Global state instance
static GLOBAL_STATE: once_cell::sync::Lazy<GlobalState> =
    once_cell::sync::Lazy::new(GlobalState::new);

/// Get global state instance
pub fn get_global_state() -> &'static GlobalState {
    &GLOBAL_STATE
}

/// Convenience function: Update slot
pub fn update_slot(slot: u64) {
    get_global_state().update_slot(slot);
}

/// Convenience function: Add developer address
pub fn add_dev_address(address: Pubkey) {
    get_global_state().add_dev_address(address);
}

/// Convenience function: Check if address is a developer address
pub fn is_dev_address(address: &Pubkey) -> bool {
    get_global_state().is_dev_address(address)
}

/// Convenience function: Add Bonk developer address
pub fn add_bonk_dev_address(address: Pubkey) {
    get_global_state().add_bonk_dev_address(address);
}

/// Convenience function: Check if address is a Bonk developer address
pub fn is_bonk_dev_address(address: &Pubkey) -> bool {
    get_global_state().is_bonk_dev_address(address)
}

/// Convenience function: Get all developer addresses
pub fn get_dev_addresses() -> Vec<Pubkey> {
    get_global_state().get_dev_addresses()
}

/// Convenience function: Get all Bonk developer addresses
pub fn get_bonk_dev_addresses() -> Vec<Pubkey> {
    get_global_state().get_bonk_dev_addresses()
}
