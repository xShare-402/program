use anchor_lang::prelude::*;
use crate::AccountClass;
use crate::base::*;
use crate::utils::*;

use anchor_lang::system_program::{create_account, CreateAccount};

use solana_program::instruction::Instruction;

pub fn optimal_bloom_parameters(n: usize, p: f64) -> (usize, usize) {
    let ln2_squared = std::f64::consts::LN_2.powi(2);
    let m = (-(n as f64) * p.ln() / ln2_squared).ceil() as usize;
    let k = ((m as f64 / n as f64) * std::f64::consts::LN_2).round() as usize;
    let ma = (m as u64) / 8;
    ((ma * 8) as usize, k)
}

const BLOOM_BITS: usize = 100_000*8;
pub const BLOOM_HASH_COUNT: usize = 11;
const BLOOM_DISCRIMINATOR:[u8;8] = [100, 183, 26, 239, 237, 36, 74, 59];
const VERIFY_BLOOM_DISCRIMINATOR:[u8;8] = [140, 110, 112, 112, 163, 39, 4, 16];
#[error_code]
pub enum BloomError {
  #[msg("Something happened.")]
  UserAlreadyInBloom
}


pub fn hash(data: &[u8], seed: u64) -> usize {
       
    if false { //keep this for testing
     return cyrb53_bytes(data, 0) as usize;
     }
 
     // Simple FNV-1a hash with seed
     let mut hash = 0xcbf29ce484222325u64 ^ seed;
     for &byte in data {
         hash ^= byte as u64;
         hash = hash.wrapping_mul(0x100000001b3);
     }
     hash as usize
 }
