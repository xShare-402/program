use mpl_bubblegum::instructions::CreateTreeConfigCpiBuilder;

use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct CreateCnft { //17
    pub name:String,
    pub symbol:String,
    pub uri:String,
    pub fee:u16,
  pub creators:Vec<Creator>,
  pub collection:Option<Collection>,
  
}

pub fn create_nft_to_metadata(create_nft:CreateCnft) -> MetadataArgs {
    return MetadataArgs {
        name:create_nft.name,
        symbol:create_nft.symbol,
        uri:create_nft.uri,
        seller_fee_basis_points: create_nft.fee,
        creators:create_nft.creators,
        primary_sale_happened: false,
        is_mutable: true,
        edition_nonce: None,
        collection: create_nft.collection,
        uses: None,
        token_standard: Some(TokenStandard::NonFungible),
        token_program_version: TokenProgramVersion::Original,
    };
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct TokenMetadata
{
  pub name: String,
  pub symbol: String,
  pub arweave:String,
  pub asset_bundler:AssetBundler
}

#[derive(Clone)]
pub struct Bubblegum;
impl anchor_lang::Id for Bubblegum {
  fn id() -> Pubkey {
    mpl_bubblegum::ID
  }
}

const NOOP_ID:[u8;32] = [11, 188, 15, 192, 187, 71, 202, 47, 116, 196, 17, 46, 148, 171, 19, 207, 163, 198, 52, 229, 220, 23, 234, 203, 3, 205, 26, 35, 205, 126, 120, 124];

#[derive(Clone)]
pub struct Noop;
impl anchor_lang::Id for Noop {
  fn id() -> Pubkey {
    Pubkey::new_from_array(NOOP_ID)
  }
}

pub const SPL_ACCOUNT_COMPRESSION:[u8;32] = [9, 42, 19, 238, 149, 196, 28, 186, 8, 166, 127, 90, 198, 126, 141, 247, 225, 218, 17, 98, 94, 29, 100, 19, 127, 143, 79, 35, 131, 3, 127, 20];

#[derive(Clone)]
pub struct SplAccountCompression;
impl anchor_lang::Id for SplAccountCompression {
  fn id() -> Pubkey {
    Pubkey::new_from_array(SPL_ACCOUNT_COMPRESSION)
  }
}

//36+14+204+2+1+1+2+2
#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct MetadataArgs { //490
    /// The name of the asset
    pub name: String, //36
    /// The symbol for the asset
    pub symbol: String, //14
    /// URI pointing to JSON representing the asset
    pub uri: String, //204
    /// Royalty basis points that goes to creators in secondary sales (0-10000)
    pub seller_fee_basis_points: u16, //2
    // Immutable, once flipped, all sales of this metadata are considered secondary.
    pub primary_sale_happened: bool, //1
    // Whether or not the data struct is mutable, default is not
    pub is_mutable: bool, //1
    /// nonce for easy calculation of editions, if present
    pub edition_nonce: Option<u8>, //2
    /// Since we cannot easily change Metadata, we add the new DataV2 fields here at the end.
    pub token_standard: Option<TokenStandard>, //2
    /// Collection
    pub collection: Option<Collection>, //34
    /// Uses
    pub uses: Option<Uses>, //18
    pub token_program_version: TokenProgramVersion, //2
    pub creators: Vec<Creator>, //174
}


#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub enum TokenProgramVersion {
    Original,
    Token2022,
}


#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub enum TokenStandard {
    NonFungible,        // This is a master edition
    FungibleAsset,      // A token with metadata that can also have attributes
    Fungible,           // A token with simple metadata
    NonFungibleEdition, // This is a limited edition
}
#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct Creator {
    pub address: Pubkey,
    pub verified: bool,
    // In percentages, NOT basis points ;) Watch out!
    pub share: u8,
}
#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub enum UseMethod {
    Burn,
    Multiple,
    Single,
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct Uses {
    // 17 bytes + Option byte
    pub use_method: UseMethod, //1
    pub remaining: u64,        //8
    pub total: u64,            //8
}

#[repr(C)]
#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct Collection {
    pub verified: bool,
    pub key: Pubkey,
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
#[repr(u8)]
pub enum AssetBundler {
  Arweave = 0,
  IrysGateway = 1,
  RawTurboDev = 2,
  Raw = 3,
}

pub fn mint_to_collection_cnft<'a>(
  bubblegum_program: &AccountInfo<'a>,
  tree_authority: &AccountInfo<'a>,
  owner: &AccountInfo<'a>,
  delegate: &AccountInfo<'a>,
  merkle_tree: &AccountInfo<'a>,
  payer: &AccountInfo<'a>,
  tree_delegate: &AccountInfo<'a>,
  collection_authority: &AccountInfo<'a>,
  collection_authority_record_pda: &AccountInfo<'a>,
  collection_mint: &AccountInfo<'a>,
  collection_metadata: &AccountInfo<'a>,
  edition_account: &AccountInfo<'a>,
  bubblegum_signer: &AccountInfo<'a>,
  log_wrapper: &AccountInfo<'a>,
  compression_program: &AccountInfo<'a>,
  token_metadata_program: &AccountInfo<'a>,
  system_program: &AccountInfo<'a>,
  metadata:MetadataArgs,
  signature:&[&[&[u8]]],
  remaining_accounts:&[AccountInfo<'a>]
) -> Result<()> {
  //153, 18, 178, 47, 197, 158, 86, 15
  
  
  let remaining_accounts_len = remaining_accounts.len();
   let mut accounts = Vec::with_capacity(
    16 // space for the 7 AccountMetas
    + remaining_accounts_len,
   );
   accounts.extend(vec![
    AccountMeta::new(tree_authority.key(), false), //tree_auth
    AccountMeta::new_readonly(owner.key(), false), //leaf_owner
    AccountMeta::new_readonly(delegate.key(), false), //leaf_delegate
    AccountMeta::new(merkle_tree.key(), false), //merkle_tree
    AccountMeta::new_readonly(payer.key(), true), //payer
    AccountMeta::new_readonly(tree_delegate.key(), true), //tree_delegate
    AccountMeta::new_readonly(collection_authority.key(), true), //collection_authority
    AccountMeta::new_readonly(collection_authority_record_pda.key(), false), //collection_authority_record_pda
    AccountMeta::new_readonly(collection_mint.key(), false), //collection_mint
    AccountMeta::new(collection_metadata.key(), false), //collection_metadata
    AccountMeta::new_readonly(edition_account.key(), false), //edition_account
    AccountMeta::new_readonly(bubblegum_signer.key(), false), //bubblegum_signer
    AccountMeta::new_readonly(log_wrapper.key(), false),
    AccountMeta::new_readonly(compression_program.key(), false),
    AccountMeta::new_readonly(token_metadata_program.key(), false),
    AccountMeta::new_readonly(system_program.key(), false),
   ]);
   
   let mint_to_collection_discriminator: [u8; 8] = [153, 18, 178, 47, 197, 158, 86, 15];
   
   let metadata_vec = metadata.try_to_vec().unwrap();
   
   
   
   let mut data = Vec::with_capacity(
     8 // The length of mint_to_collection_discriminator,
     + metadata_vec.len()
  );
  
  data.extend(mint_to_collection_discriminator);
  data.extend(metadata_vec);
  
  let mut account_infos = Vec::with_capacity(
    16 // space for the 7 AccountInfos
    + remaining_accounts_len,
   );
   
   account_infos.extend(vec![
     tree_authority.clone(), //tree_auth
     owner.clone(), //leaf_owner
     delegate.clone(), //leaf_delegate
     merkle_tree.clone(), //merkle_tree
     payer.clone(), //payer
     tree_delegate.clone(), //tree_delegate
     collection_authority.clone(), //collection_authority
     collection_authority_record_pda.clone(), //collection_authority_record_pda
     collection_mint.clone(), //collection_mint
     collection_metadata.clone(), //collection_metadata
     edition_account.clone(), //edition_account
     bubblegum_signer.clone(), //bubblegum_signer
     log_wrapper.clone(),
     compression_program.clone(),
     token_metadata_program.clone(),
     system_program.clone(),
    ]);
    
    // Add "accounts" (hashes) that make up the merkle proof from the remaining accounts.
     for acc in remaining_accounts.iter() {
      accounts.push(AccountMeta::new_readonly(acc.key(), true));
      account_infos.push(acc.clone());
     }
     
   let instruction = solana_program::instruction::Instruction {
     program_id: bubblegum_program.key(),
     accounts,
     data,
    };
    
   //let outer = &[signature.as_slice()];
     
   let acc2 = account_infos.clone();
   solana_program::program::invoke_signed(&instruction, &acc2[..], signature)?;
   
  
  Ok(())
}

pub fn verify_leaf<'a>(
  compression_program: &AccountInfo<'a>,
  merkle_tree: &AccountInfo<'a>,
  leaf_hash:[u8;32],
  root_hash:[u8;32],
  index:u32,
  remaining_accounts:&[AccountInfo<'a>]
) -> Result<()> {
  
  
  let remaining_accounts_len = remaining_accounts.len();
   let mut accounts = Vec::with_capacity(
    1 // space for the 1 AccountMetas
    + remaining_accounts_len,
   );
   accounts.extend(vec![
    AccountMeta::new_readonly(merkle_tree.key(), false), //merkle_tree
   ]);
   
   let mint_discriminator: [u8; 8] = [124, 220,  22, 223, 104,  10, 250, 224];
   
   let mut data:Vec<u8> = Vec::with_capacity(
      8 // The length of mint_to_collection_discriminator,
      + 32
      + 32
      + 4
   );
   
   data.extend(mint_discriminator);
   data.extend(&root_hash);
   data.extend(&leaf_hash);
   data.extend(&index.to_le_bytes());
   
   let mut account_infos = Vec::with_capacity(
     1 // space for the 7 AccountInfos
     + remaining_accounts_len,
    );
    
    account_infos.extend(vec![
      merkle_tree.clone()
     ]);
     
     for acc in remaining_accounts.iter() {
         accounts.push(AccountMeta::new_readonly(acc.key(), false));
         account_infos.push(acc.clone());
        }
        
      let instruction = solana_program::instruction::Instruction {
        program_id: compression_program.key(),
        accounts,
        data,
       };
       
      let acc2 = account_infos.clone();
      let result = solana_program::program::invoke(&instruction, &acc2[..]);
      
      match result {
      Ok(()) => {
      }
      Err(_) => {
         // Handle error case  
         return Err(ProgramError::InvalidArgument.into())
       }
      } 
  
 Ok(()) 
}

pub fn mint_cnft<'a>(
  bubblegum_program: &AccountInfo<'a>,
  tree_authority: &AccountInfo<'a>,
  leaf_delegate: &AccountInfo<'a>,
  leaf_owner: &AccountInfo<'a>,
  merkle_tree: &AccountInfo<'a>,
  payer: &AccountInfo<'a>,
  tree_delegate: &AccountInfo<'a>,
  log_wrapper: &AccountInfo<'a>,
  compression_program: &AccountInfo<'a>,
  system_program: &AccountInfo<'a>,
  metadata:MetadataArgs,
  signature:&[&[&[u8]]],
  remaining_accounts:&[AccountInfo<'a>]
) -> Result<()> {
  //153, 18, 178, 47, 197, 158, 86, 15
  
  
  let remaining_accounts_len = remaining_accounts.len();
   let mut accounts = Vec::with_capacity(
    9 // space for the 7 AccountMetas
    + remaining_accounts_len,
   );
   accounts.extend(vec![
    AccountMeta::new(tree_authority.key(), false), //tree_auth
    AccountMeta::new_readonly(leaf_owner.key(), false), //leaf_owner
    AccountMeta::new_readonly(leaf_delegate.key(), false), //leaf_delegate
    AccountMeta::new(merkle_tree.key(), false), //merkle_tree
    AccountMeta::new_readonly(payer.key(), true), //payer
    AccountMeta::new_readonly(tree_delegate.key(), true), //tree_delegate
    AccountMeta::new_readonly(log_wrapper.key(), false),
    AccountMeta::new_readonly(compression_program.key(), false),
    AccountMeta::new_readonly(system_program.key(), false),
   ]);
   
   let mint_discriminator: [u8; 8] = [145, 98, 192, 118, 184, 147, 118, 104];
   
   let metadata_vec = metadata.try_to_vec().unwrap();
   
   
   
   let mut data = Vec::with_capacity(
     8 // The length of mint_to_collection_discriminator,
     + metadata_vec.len()
  );
  
  data.extend(mint_discriminator);
  data.extend(metadata_vec);
  
  let mut account_infos = Vec::with_capacity(
    10 // space for the 7 AccountInfos
    + remaining_accounts_len,
   );
   
   account_infos.extend(vec![
     tree_authority.clone(), //tree_auth
     leaf_owner.clone(), //leaf_owner
     leaf_delegate.clone(), //leaf_delegate
     merkle_tree.clone(), //merkle_tree
     payer.clone(), //payer
     tree_delegate.clone(), //tree_delegate
     log_wrapper.clone(),
     compression_program.clone(),
     system_program.clone(),
    ]);
  
    // Add "accounts" (hashes) that make up the merkle proof from the remaining accounts.
     for acc in remaining_accounts.iter() {
      accounts.push(AccountMeta::new_readonly(acc.key(), true));
      account_infos.push(acc.clone());
     }
     
   let instruction = solana_program::instruction::Instruction {
     program_id: bubblegum_program.key(),
     accounts,
     data,
    };
    
   let acc2 = account_infos.clone();
   solana_program::program::invoke_signed(&instruction, &acc2[..], signature)?;
  
  Ok(())
}

pub fn burn_cnft<'a>(
   tree_authority: &AccountInfo<'a>,
   owner: &AccountInfo<'a>,
   delegate: &AccountInfo<'a>,
   merkle_tree: &AccountInfo<'a>,
   log_wrapper: &AccountInfo<'a>,
   compression_program: &AccountInfo<'a>,
   system_program: &AccountInfo<'a>,
   bubblegum_program: &AccountInfo<'a>,
   root:[u8; 32],
   data_hash:[u8; 32],
   creator_hash:[u8; 32],
   nonce:u64,
   index:u32,
   signature:Vec<&[u8]>,
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
    AccountMeta::new_readonly(tree_authority.key(), false),
    AccountMeta::new_readonly(owner.key(), false),
    AccountMeta::new_readonly(delegate.key(), true), //leaf_delegate
    AccountMeta::new(merkle_tree.key(), false),
    AccountMeta::new_readonly(log_wrapper.key(), false),
    AccountMeta::new_readonly(compression_program.key(), false),
    AccountMeta::new_readonly(system_program.key(), false),
   ]);
   
   let burn_discriminator: [u8; 8] = [116, 110, 29, 56, 107, 219, 42, 93];
   
   let mut data = Vec::with_capacity(
    8 // The length of burn_discriminator,
    + root.len()
    + data_hash.len()
    + creator_hash.len()
    + 8 // The length of the nonce
    + 4, // The length of the index
   );
   
   data.extend(burn_discriminator);
   data.extend(root);
   data.extend(data_hash);
   data.extend(creator_hash);
   data.extend(nonce.to_le_bytes());
   data.extend(index.to_le_bytes());
   
   let mut account_infos = Vec::with_capacity(
    7 // space for the 7 AccountInfos
    + remaining_accounts_len,
   );
   
   //let tree_auth_clone = tree_authority.clone();
   
   account_infos.extend(vec![
    tree_authority.clone(),
    owner.clone(),
    delegate.clone(), //leaf delegate
    merkle_tree.clone(),
    log_wrapper.clone(),
    compression_program.clone(),
    system_program.clone(),
   ]);
   
   // Add "accounts" (hashes) that make up the merkle proof from the remaining accounts.
   for acc in remaining_accounts.iter() {
    accounts.push(AccountMeta::new_readonly(acc.key(), false));
    account_infos.push(acc.clone());
   }
   
   let instruction = solana_program::instruction::Instruction {
    program_id: bubblegum_program.key(),
    accounts,
    data,
   };
   
   
   /*let pack_bump_vector = pay_bump.to_le_bytes(); //PDA that I use to delegate the leaf
    let inner2 = vec![
      b"pack_authority".as_ref(),
      pack_acount_key.as_ref(),
      &pack_bump_vector
     ];*/
   
   let outer = &[signature.as_slice()];
   
   let acc2 = account_infos.clone();
   
   solana_program::program::invoke_signed(&instruction, &acc2[..], outer)?;
   
   
   /*** ENDS BURN ***/
   
   Ok(())
}