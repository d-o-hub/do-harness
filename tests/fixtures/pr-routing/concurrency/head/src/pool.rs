use std::sync::Mutex;
static M: Mutex<u32> = Mutex::new(0);
pub fn get() -> u32 { *M.lock().expect("poisoned") }
