use std::sync::OnceLock;

pub type Round = u64;
pub type Wave = u64;
pub type Epoch = u64;
pub type Id = usize;
pub type Stake = u32;
pub type WorkerId = u32;

pub type RandomNum = u64;

pub static BEACON_PER_EPOCH: OnceLock<u64> = OnceLock::new();
pub static MAX_WAVE: OnceLock<u64> = OnceLock::new();
pub static MAX_EPOCH: OnceLock<u64> = OnceLock::new();

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
// pub(crate) fn is_last_round_in_wave(round: &Round) -> bool {
//     let max_wave = MAX_WAVE.get().unwrap();
//     round % max_wave == 0
// }