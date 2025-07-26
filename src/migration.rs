use std::sync::atomic::{AtomicU64, Ordering};

use crate::worker::WorkerId;

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum Mode {
    None,
    Push,
    Pull,
}

fn pack(target: usize, mode: Mode, balance: usize) -> u64 {
    debug_assert!(target <= 0xFFFF_FFFF, "target too large");
    debug_assert!(balance <= 0x3FFF_FFFF, "balance too large");

    let target = target as u64;
    let balance = balance as u64;
    let mode = mode as u64;

    (target & 0xFFFF_FFFF) |        // lower 32 bits
    ((mode & 0b11) << 32) |         // 2 bits for mode
    ((balance & 0x3FFF_FFFF) << 34) // upper 30 bits
}

fn unpack(word: u64) -> (usize, Mode, usize) {
    let target = word & 0xFFFF_FFFF;
    let mode = match (word >> 32) & 0b11 {
        0 => Mode::None,
        1 => Mode::Push,
        2 => Mode::Pull,
        _ => unreachable!(),
    };
    let balance = (word >> 34) & 0x3FFF_FFFF;
    (target as usize, mode, balance as usize)
}

#[derive(Copy, Clone, Debug)]
pub struct Parameters {
    pub target: WorkerId,
    pub mode: Mode,
    pub balance: usize,
}

impl Parameters {
    pub const fn none() -> Self {
        Parameters {
            target: 0,
            mode: Mode::None,
            balance: 0,
        }
    }
}

pub struct Migration {
    packed: AtomicU64,
}

impl Migration {
    pub fn new() -> Self {
        Self {
            packed: AtomicU64::new(0),
        }
    }

    pub fn store(&self, parameters: Parameters) {
        let packed = pack(
            parameters.target as usize,
            parameters.mode,
            parameters.balance,
        );
        self.packed.store(packed as u64, Ordering::Release);
    }

    pub fn load(&self) -> Parameters {
        let packed = self.packed.load(Ordering::Acquire);
        let (target, mode, balance) = unpack(packed);

        Parameters {
            target,
            mode,
            balance,
        }
    }

    pub fn load_for_push(&self) -> Parameters {
        loop {
            let packed = self.packed.load(Ordering::Acquire);
            let (target, mode, balance) = unpack(packed);

            let new_mode = match mode {
                Mode::Push => Mode::None,
                _ => mode,
            };

            // If mode doesn't need changing, return immediately
            if new_mode == mode {
                return Parameters {
                    target,
                    mode,
                    balance,
                };
            }

            // Otherwise try to reset Push -> None
            let new_packed = pack(target, new_mode, balance);

            match self.packed.compare_exchange(
                packed,
                new_packed,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Parameters {
                        target,
                        mode,
                        balance,
                    };
                }
                Err(_) => {
                    // Another thread changed it, retry
                    continue;
                }
            }
        }
    }
}

impl std::fmt::Debug for Migration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Parameters {
            target,
            mode,
            balance,
        } = self.load_for_push();
        f.debug_struct("Migration")
            .field("target", &target)
            .field("mode", &mode)
            .field("balance", &balance)
            .finish()
    }
}
