use std::sync::OnceLock;

pub type Round = u64;
pub type Wave = u64;
pub type Epoch = u64;
pub type Id = usize;
pub type Stake = u32;
pub type WorkerId = u32;

pub type RandomNum = u128;
#[cfg(feature = "pq")]
pub type ZqMod = u128;

pub static BEACON_PER_EPOCH: OnceLock<u64> = OnceLock::new();
pub static MAX_WAVE: OnceLock<u64> = OnceLock::new();
pub static MAX_EPOCH: OnceLock<u64> = OnceLock::new();
#[cfg(feature = "pq")]
pub static MAX_INDEX: OnceLock<usize> = OnceLock::new();

/// The default channel capacity.
pub const CHANNEL_CAPACITY: usize = 1_000;

pub fn get_wave_by_round(round: Round) -> Wave {
    let max_epoch = MAX_EPOCH.get().unwrap();
    let max_wave = MAX_WAVE.get().unwrap();
    
    let remainder = round % (max_epoch * max_wave);
    if remainder == 0 {
        *max_epoch
    } else {
        (remainder - 1) / max_wave + 1
    }
}
pub fn get_epoch_by_round(round: Round) -> Epoch {
    let max_epoch = MAX_EPOCH.get().unwrap();
    let max_wave = MAX_WAVE.get().unwrap();
    (round - 1) / (max_epoch * max_wave) + 1
}

pub fn get_epoch_by_wave(wave: Wave) -> Epoch {
    let max_epoch = MAX_EPOCH.get().unwrap();
    (wave - 1) / (max_epoch ) + 1
}

pub fn get_round_by_epoch_wave(epoch: Epoch, wave: Wave) -> Round {
    let max_epoch = MAX_EPOCH.get().unwrap();
    let max_wave = MAX_WAVE.get().unwrap();

    // Validate inputs
    if wave > *max_wave || epoch == 0 || wave == 0 {
        panic!("Invalid wave or epoch value");
    }

    // Calculate the base round for the start of this epoch
    let epoch_base = (epoch - 1) * (max_epoch * max_wave);
    // Add the rounds for the waves within this epoch
    let wave_offset = (wave - 1) * max_wave + 1;

    epoch_base + wave_offset
}

pub fn round_to_epoch_index(round: Round, max_epoch: u64) -> (Epoch, usize) {
    assert!(round > 0, "round must be positive");
    assert!(max_epoch > 0, "max_epoch must be positive");

    let rounds_per_epoch = 2 * max_epoch;
    let epoch = (round - 1) / rounds_per_epoch + 1;
    let index = (round - 1) % rounds_per_epoch + 1;

    (epoch, index as usize)
}


pub fn leader_round_to_epoch_index(round: Round, max_epoch: u64) -> (Epoch, usize) {
    assert_eq!(round % 2, 1, "round must be an odd number");
    assert!(round > 0, "round must be positive");
    assert!(max_epoch > 0, "max_epoch must be positive");

    let rounds_per_epoch = 2 * max_epoch;
    let epoch = (round - 1) / rounds_per_epoch + 1;
    let index = ((round - 1) % rounds_per_epoch) / 2 + 1;

    (epoch, index as usize)
}

pub fn epoch_index_to_leader_round(epoch: Epoch, index: usize, max_epoch: u64) -> Round {
    assert!(epoch > 0, "epoch must be positive");
    assert!(index > 0 && index <= max_epoch as usize, "index must be between 1 and max_epoch");
    assert!(max_epoch > 0, "max_epoch must be positive");

    let rounds_per_epoch = 2 * max_epoch;
    let round = (epoch - 1) * rounds_per_epoch + (2 * index as u64 - 1);

    round
}