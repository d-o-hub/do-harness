use std::sync::{Arc, RwLock};

pub struct Slot {
    id: usize,
    checked_out: bool,
}

pub struct Pool {
    slots: Arc<RwLock<Vec<Slot>>>,
}

impl Pool {
    pub fn capacity(&self) -> usize {
        let slots = self.slots.read().expect("pool lock poisoned");
        slots.len()
    }

    pub fn acquire_01(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 1)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_02(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 2)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_03(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 3)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_04(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 4)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_05(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 5)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_06(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 6)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_07(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 7)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_08(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 8)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_09(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 9)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_10(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 10)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_11(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 11)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_12(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 12)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_13(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 13)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_14(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 14)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_15(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 15)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_16(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 16)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_17(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 17)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_18(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 18)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_19(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 19)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_20(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 20)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_21(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 21)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_22(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 22)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_23(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 23)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_24(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 24)?;
        slot.checked_out = true;
        Some(slot.id)
    }

    pub fn acquire_25(&self) -> Option<usize> {
        let mut slots = self.slots.write().expect("pool lock poisoned");
        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id % 26 == 25)?;
        slot.checked_out = true;
        Some(slot.id)
    }

}
