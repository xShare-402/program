use anchor_lang::prelude::*;
use crate::base::*;
use brine_ed25519::sig_verify;
use anchor_lang::solana_program::program_memory::sol_memset;

pub fn verify_signature(
    message: &[u8],       // e.g., "action" + nonce
    signature: &[u8],     // must be exactly 64 bytes
    pubkey_bytes: &[u8],  // must be exactly 32 bytes
) -> Result<()> {
    let verif = sig_verify(pubkey_bytes, signature, message);

     match verif {
      Ok(()) => {
      }
      Err(err) => {
        msg!("err sig {:?}",err);
         return Err(GeneralError::GeneralError.into())
      }
    }

  Ok(())
}

pub fn close_account<'a>(
    source_account: &AccountInfo<'a>,
    receiver_account: &AccountInfo<'a>,
) -> Result<()> {
    let current_lamports = source_account.lamports();
    let account_data_size = source_account.data_len();

    **source_account.lamports.borrow_mut() = 0;
    **receiver_account.lamports.borrow_mut() = receiver_account
        .lamports()
        .checked_add(current_lamports)
        .ok_or(ProgramError::InvalidArgument)?;

    #[allow(clippy::explicit_auto_deref)]
    sol_memset(*source_account.try_borrow_mut_data()?, 0, account_data_size);

    Ok(())
}

pub fn pay_to_user<'a>(
  from_info:&AccountInfo<'a>,
  to_info:&AccountInfo<'a>,
  amount:u64
) -> Result<()> {
  let to_lamports_initial = to_info.lamports();
  let from_lamports_initial = from_info.lamports();
	let final_from_amount = from_lamports_initial - amount;
	**to_info.lamports.borrow_mut() = to_lamports_initial + amount;
	**from_info.lamports.borrow_mut() = final_from_amount;
  Ok(())
}

pub fn bytes_to_string(bytes: &[u8]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..len]).to_string()
}

pub fn hash_to_u8_array(hash: solana_program::keccak::Hash) -> [u8; 32] {
  let mut result = [0; 32];
  result.copy_from_slice(&hash.to_bytes()[..32]);
  result
}

const METABYTES:[u8;32] = [11, 112, 101, 177, 227, 209, 124, 69, 56, 157, 82, 127, 107, 4, 195, 205, 88, 184, 108, 115, 26, 160, 253, 181, 73, 182, 209, 188, 3, 248, 41, 70];

pub fn cyrb53_bytes(key: &[u8], seed: u32) -> u64 {
    const A: u32 = 2654435761;
    const B: u32 = 1597334677;
    const C: u32 = 2246822507;
    const D: u32 = 3266489909;
    const E: u64 = 4294967296;
    const F: u64 = 2097151;
  
    let mut h1: u32 = 0xdeadbeef ^ seed;
    let mut h2: u32 = 0x41c6ce57 ^ seed;
  
    for byte in key {
    let byte_code = *byte as u32;
    h1 = (h1 ^ byte_code).wrapping_mul(A);
    h2 = (h2 ^ byte_code).wrapping_mul(B);
    }
    
    h1 = (h1 ^ (h1 >> 16)).wrapping_mul(C) ^ (h2 ^ (h2 >> 13)).wrapping_mul(D);
    h2 = (h2 ^ (h2 >> 16)).wrapping_mul(C) ^ (h1 ^ (h1 >> 13)).wrapping_mul(D);
  
    E * (F & (h2 as u64)) + ((h1 >> 0) as u64)
  }




  pub fn create_master_edition_v3<'a>(
   edition: &AccountInfo<'a>,
   mint: &AccountInfo<'a>,
   update_authority: &AccountInfo<'a>,
   mint_authority: &AccountInfo<'a>,
   metadata: &AccountInfo<'a>,
   payer: &AccountInfo<'a>,
   rent: &AccountInfo<'a>,
   system_program: &AccountInfo<'a>,
   token_program: &AccountInfo<'a>,
   supply:u64,
   signature:&[&[&[u8]]],
   remaining_accounts:&[AccountInfo<'a>]
) -> Result<()> {
   
   /*** STARTS BURN ***/
   //root:[u8; 32], data_hash:[u8; 32], creator_hash:[u8; 32], nonce:u64, index:u32
   
   
   let remaining_accounts_len = remaining_accounts.len();
   let mut accounts = Vec::with_capacity(
    9 // space for the 7 AccountMetas
    + remaining_accounts_len,
   );
   
   accounts.extend(vec![
    AccountMeta::new(edition.key(), false),
    AccountMeta::new(mint.key(), false),
    AccountMeta::new_readonly(update_authority.key(), true),
    AccountMeta::new_readonly(mint_authority.key(), true),
    AccountMeta::new(payer.key(), true),
    AccountMeta::new(metadata.key(), false),
    AccountMeta::new_readonly(token_program.key(), false),
    AccountMeta::new_readonly(system_program.key(), false),
    AccountMeta::new_readonly(rent.key(), false),
   ]);
   
   let discriminator: [u8; 1] = [17];
   
   let mut data = Vec::with_capacity(
    1 // The length of burn_discriminator
    + 9
   );
   
   let supply_bytes = supply.to_le_bytes();
   
   data.extend(discriminator);
   data.extend(&[1]);
   data.extend(&supply_bytes);

   let mut account_infos = Vec::with_capacity(
    9 // space for the 7 AccountInfos
    + remaining_accounts_len,
   );
   
   //let tree_auth_clone = tree_authority.clone();
   
   account_infos.extend(vec![
    edition.clone(),
    mint.clone(),
    update_authority.clone(),
    mint_authority.clone(),
    payer.clone(),
    metadata.clone(),
    token_program.clone(),
    system_program.clone(),
    rent.clone(),
   ]);
   
   // Add "accounts" (hashes) that make up the merkle proof from the remaining accounts.
   for acc in remaining_accounts.iter() {
    accounts.push(AccountMeta::new_readonly(acc.key(), false));
    account_infos.push(acc.clone());
   }
   
   let metapubkey = Pubkey::new_from_array(METABYTES);
   let instruction = solana_program::instruction::Instruction {
    program_id: metapubkey,
    accounts,
    data,
   };
   
   let acc2 = account_infos.clone();
   
   solana_program::program::invoke_signed(&instruction, &acc2[..], signature)?;
   
   /*** ENDS BURN ***/
   
   Ok(())
}

pub fn create_metadata_v3<'a>(
   metadata: &AccountInfo<'a>,
   mint: &AccountInfo<'a>,
   mint_authority: &AccountInfo<'a>,
   payer: &AccountInfo<'a>,
   update_authority: &AccountInfo<'a>,
   rent: &AccountInfo<'a>,
   system_program: &AccountInfo<'a>,
   name:String,
   symbol:String,
   uri:String,
   share:u16,
   collection:Option<bool>,
   signature:&[&[&[u8]]],
   remaining_accounts:&[AccountInfo<'a>]
) -> Result<()> {
   
   /*** STARTS BURN ***/
   //root:[u8; 32], data_hash:[u8; 32], creator_hash:[u8; 32], nonce:u64, index:u32
   
   
   let remaining_accounts_len = remaining_accounts.len();
   let mut accounts = Vec::with_capacity(
    7 // space for the 7 AccountMetas
    + remaining_accounts_len,
   );
   
   accounts.extend(vec![
    AccountMeta::new(metadata.key(), false),
    AccountMeta::new_readonly(mint.key(), false),
    AccountMeta::new_readonly(mint_authority.key(), true), //mint_authority
    AccountMeta::new(payer.key(), true),
    AccountMeta::new_readonly(update_authority.key(), true),
    AccountMeta::new_readonly(system_program.key(), false),
    AccountMeta::new_readonly(rent.key(), false),
   ]);
   
   let discriminator: [u8; 1] = [33];
   
   let name_vec = name.try_to_vec().unwrap();
   let symbol_vec = symbol.try_to_vec().unwrap();
   let uri_vec = uri.try_to_vec().unwrap();
   let share_vec = share.to_le_bytes();
   
   let mut data = Vec::with_capacity(
    1 // The length of burn_discriminator,
    + name_vec.len()
    + symbol_vec.len()
    + uri_vec.len()
    + 2 //seller fee basis
    + 1 + if collection.is_some() { 4 + 32 + 1 + 1 } else { 0 } //creators
    + 1 //empty collection
    + 1 //empty uses
    + 1 //mutable
    + 1 + if collection.is_some() { 8 } else { 0 } //collection details
   );
   let payer_key = payer.key();
   data.extend(discriminator);
   data.extend(&name_vec);
   data.extend(&symbol_vec);
   data.extend(&uri_vec);
   data.extend(&share_vec);
   if let Some(collection) = collection {
     data.extend(&[1]); //creators on
     data.extend(&[1,0,0,0]); //creators size
     data.extend(&payer_key.to_bytes()); //creators size
     data.extend(&[0,100]); //verified,share
   } else {
     data.extend(&[0]); //creators off
   }
   data.extend(&[0,0,1]); //collection,uses,mutable
   
   if let Some(collection) = collection { //collectionDetails
      data.extend(&[1,0,0,0,0,0,0,0,0,0]);
    } else {
      data.extend(&[0]);
    }
   
   
   let mut account_infos = Vec::with_capacity(
    7 // space for the 7 AccountInfos
    + remaining_accounts_len,
   );
   
   //let tree_auth_clone = tree_authority.clone();
   
   account_infos.extend(vec![
    metadata.clone(),
    mint.clone(),
    mint_authority.clone(), //leaf delegate
    payer.clone(),
    update_authority.clone(),
    system_program.clone(),
    rent.clone()
   ]);
   
   // Add "accounts" (hashes) that make up the merkle proof from the remaining accounts.
   for acc in remaining_accounts.iter() {
    accounts.push(AccountMeta::new_readonly(acc.key(), false));
    account_infos.push(acc.clone());
   }
   
   let metapubkey = Pubkey::new_from_array(METABYTES);
   let instruction = solana_program::instruction::Instruction {
    program_id: metapubkey,
    accounts,
    data,
   };
   
   
   let acc2 = account_infos.clone();
   
   solana_program::program::invoke_signed(&instruction, &acc2[..], signature)?;
   
   /*** ENDS BURN ***/
   
   Ok(())
}



const ASCII_SAFE: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ[]abcdefghijklmnopqrstuvwxyz{|}~";
const ASCII_STRING: &str = "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

const ASCII_STRING_NP: &str = "\"!#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|";

const INCREASER_91: &str = "}";
const INCREASER_182: &str = "~";

const INCREASER_65: &str = "}";
const INCREASER_130: &str = "~";
const INCREASER_195: &str = "|";

pub fn encode_u64_to_ascii_string(value: u64) -> String {
  let base = ASCII_STRING.len() as u64;
  let mut result = String::new();
  let mut num = value;

  while num > 0 {
    let remainder = (num % base) as usize;
    result.insert(0, ASCII_STRING.chars().nth(remainder).unwrap());
    num /= base;
  }

  result
}

pub fn encode_u64_to_ascii_string_safe(value: u64) -> String {
  let base = ASCII_SAFE.len() as u64;
  let mut result = String::new();
  let mut num = value;

  while num > 0 {
    let remainder = (num % base) as usize;
    result.insert(0, ASCII_SAFE.chars().nth(remainder).unwrap());
    num /= base;
  }

  result
}


pub fn encode_bytes_to_ascii_string(bytes: &[u8], safe:bool) -> String {
   let dict = if safe { ASCII_SAFE.split_at(65).0 } else { ASCII_STRING_NP };
  
  let mut result = String::new();

  for byte in bytes {
    let mut value = *byte as usize;
    if !safe {
      if value >= 182 {
        result.push(INCREASER_182.chars().nth(0).unwrap());
        value -= 182;
      }
      if value >= 91 {
        result.push(INCREASER_91.chars().nth(0).unwrap());
        value -= 91;
      }
    } else {
      if value >= 195 {
        result.push(INCREASER_195.chars().nth(0).unwrap());
        value -= 195;
      }
      if value >= 130 {
        result.push(INCREASER_130.chars().nth(0).unwrap());
        value -= 130;
      }
      if value >= 65 {
        result.push(INCREASER_65.chars().nth(0).unwrap());
        value -= 65;
      }
    }
    result.push(dict.chars().nth(value).unwrap());
  }

  result
}