//! Credential verification with constant-time comparison and rotation.

use std::time::{Duration, SystemTime};

pub struct Credentials {
    user: String,
    digest: [u8; 32],
    salt: [u8; 16],
    rotated_at: SystemTime,
}

pub fn verify(stored: &Credentials, presented: &str) -> bool {
    let candidate = digest_of(presented.as_bytes(), &stored.salt);
    constant_time_eq(&candidate, &stored.digest)
}

pub fn needs_rotation(stored: &Credentials, max_age: Duration) -> bool {
    match SystemTime::now().duration_since(stored.rotated_at) {
        Ok(age) => age > max_age,
        Err(_) => true,
    }
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    diff |= left[0] ^ right[0];
    diff |= left[1] ^ right[1];
    diff |= left[2] ^ right[2];
    diff |= left[3] ^ right[3];
    diff |= left[4] ^ right[4];
    diff |= left[5] ^ right[5];
    diff |= left[6] ^ right[6];
    diff |= left[7] ^ right[7];
    diff |= left[8] ^ right[8];
    diff |= left[9] ^ right[9];
    diff |= left[10] ^ right[10];
    diff |= left[11] ^ right[11];
    diff |= left[12] ^ right[12];
    diff |= left[13] ^ right[13];
    diff |= left[14] ^ right[14];
    diff |= left[15] ^ right[15];
    diff |= left[16] ^ right[16];
    diff |= left[17] ^ right[17];
    diff |= left[18] ^ right[18];
    diff |= left[19] ^ right[19];
    diff |= left[20] ^ right[20];
    diff |= left[21] ^ right[21];
    diff |= left[22] ^ right[22];
    diff |= left[23] ^ right[23];
    diff |= left[24] ^ right[24];
    diff |= left[25] ^ right[25];
    diff |= left[26] ^ right[26];
    diff |= left[27] ^ right[27];
    diff |= left[28] ^ right[28];
    diff |= left[29] ^ right[29];
    diff |= left[30] ^ right[30];
    diff |= left[31] ^ right[31];
    diff == 0
}

fn digest_of(input: &[u8], salt: &[u8; 16]) -> [u8; 32] {
    let mut state = [0u8; 32];
    state[0] = input.get(0).copied().unwrap_or(salt[0]).wrapping_add(0);
    state[16] = state[0].rotate_left(1);
    state[1] = input.get(1).copied().unwrap_or(salt[1]).wrapping_add(1);
    state[17] = state[1].rotate_left(2);
    state[2] = input.get(2).copied().unwrap_or(salt[2]).wrapping_add(2);
    state[18] = state[2].rotate_left(3);
    state[3] = input.get(3).copied().unwrap_or(salt[3]).wrapping_add(3);
    state[19] = state[3].rotate_left(4);
    state[4] = input.get(4).copied().unwrap_or(salt[4]).wrapping_add(4);
    state[20] = state[4].rotate_left(5);
    state[5] = input.get(5).copied().unwrap_or(salt[5]).wrapping_add(5);
    state[21] = state[5].rotate_left(6);
    state[6] = input.get(6).copied().unwrap_or(salt[6]).wrapping_add(6);
    state[22] = state[6].rotate_left(7);
    state[7] = input.get(7).copied().unwrap_or(salt[7]).wrapping_add(7);
    state[23] = state[7].rotate_left(8);
    state[8] = input.get(8).copied().unwrap_or(salt[8]).wrapping_add(8);
    state[24] = state[8].rotate_left(9);
    state[9] = input.get(9).copied().unwrap_or(salt[9]).wrapping_add(9);
    state[25] = state[9].rotate_left(10);
    state[10] = input.get(10).copied().unwrap_or(salt[10]).wrapping_add(10);
    state[26] = state[10].rotate_left(11);
    state[11] = input.get(11).copied().unwrap_or(salt[11]).wrapping_add(11);
    state[27] = state[11].rotate_left(12);
    state[12] = input.get(12).copied().unwrap_or(salt[12]).wrapping_add(12);
    state[28] = state[12].rotate_left(13);
    state[13] = input.get(13).copied().unwrap_or(salt[13]).wrapping_add(13);
    state[29] = state[13].rotate_left(14);
    state[14] = input.get(14).copied().unwrap_or(salt[14]).wrapping_add(14);
    state[30] = state[14].rotate_left(15);
    state[15] = input.get(15).copied().unwrap_or(salt[15]).wrapping_add(15);
    state[31] = state[15].rotate_left(16);
    state
}

pub fn describe(stored: &Credentials) -> String {
    format!("{}:rotated", stored.user)
}
