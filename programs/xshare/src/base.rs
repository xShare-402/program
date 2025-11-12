use anchor_lang::prelude::*;
use crate::AccountClass;


use crate::cnft::{AssetBundler,SPL_ACCOUNT_COMPRESSION,SplAccountCompression,Noop};
use crate::cnft::Bubblegum;
use mpl_bubblegum::instructions::CreateTreeConfigCpiBuilder;
use crate::utils::*;
use mpl_bubblegum::types::LeafSchema;
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{extension::{metadata_pointer::MetadataPointer, ExtensionType, non_transferable::NonTransferable}, pod::PodMint},
        InitializeMint2,
    },
    token::Token,
    token_interface::{non_transferable_mint_initialize, metadata_pointer_initialize, TokenMetadataInitialize, token_metadata_initialize, MetadataPointerInitialize, NonTransferableMintInitialize,Mint, MintTo, TokenAccount, mint_to, Token2022, burn, Burn},
};
use anchor_spl::associated_token::AssociatedToken;




const MPL_METADATA_ID:[u8;32] = [11, 112, 101, 177, 227, 209, 124, 69, 56, 157, 82, 127, 107, 4, 195, 205, 88, 184, 108, 115, 26, 160, 253, 181, 73, 182, 209, 188, 3, 248, 41, 70];

#[derive(Clone)]
pub struct MplTokenMetadata;

impl anchor_lang::Id for MplTokenMetadata {
    fn id() -> Pubkey {
        Pubkey::new_from_array(MPL_METADATA_ID)
    }
}


use crate::utils::*;

#[error_code]
pub enum GeneralError {
  #[msg("Something happened.")]
  GeneralError
}


pub mod base_ix {
    use super::*;

    pub fn feed_global_tree(ctx: Context<FeedGlobalTree>, max_depth:u32, max_buffer_size:u32, public:bool) -> Result<()> {
    
    
    let merkle_account =  &mut ctx.accounts.merkle_account;
    
    let merkle_tree = merkle_account.clone().to_account_info();
    
    let payer = ctx.accounts.payer.to_account_info();
    
    //let lut = ctx.accounts.payer.to_account_info();
    
    //PDA
    let tree_authority = ctx.accounts.tree_authority.to_account_info();
    
    
    let system_program = ctx.accounts.system_program.to_account_info();
    
    let compression_program = ctx.accounts.compression_program.to_account_info();
    let log_wrapper = ctx.accounts.log_wrapper.to_account_info();
    
    let merkle_manager = &mut ctx.accounts.merkle_manager;
    let merkle_manager_info = merkle_manager.to_account_info();
    
    let bubblegum_program = ctx.accounts.bubblegum_program.to_account_info();
    
    let bump_vector = &ctx.bumps.merkle_manager.to_le_bytes();
    
    let inner = vec![
    b"tree".as_ref(),
    bump_vector,
    ];
    let outer = vec![inner.as_slice()];
    
   
   
   let cpi_tree = CreateTreeConfigCpiBuilder::new(&bubblegum_program)
     .tree_config(&tree_authority)
     .merkle_tree(&merkle_tree)
     .payer(&payer)
     .tree_creator(&merkle_manager_info)
     .log_wrapper(&log_wrapper)
     .compression_program(&compression_program)
     .system_program(&system_program)
     .max_depth(max_depth)
     .max_buffer_size(max_buffer_size)
     .public(public).invoke_signed(outer.as_slice());
   
   
   match cpi_tree {
     Ok(()) => {},
     Err(err) =>{
     msg!("Err {:?}",err);
     return Err(ProgramError::InvalidArgument.into())
     }
   }
   
   
    /*
    let result = create_tree(
    &bubblegum_program, 
    &tree_authority, 
    &merkle_tree, 
    &payer.clone(), 
    &merkle_manager_info, 
    &log_wrapper, 
    &compression_program, 
    &system_program, 
    &outer, 
    &[payer],
    max_depth,
    max_buffer_size
    );
    
    match result {
    Ok(()) => {},
    Err(err) => return Err(err),
    }
    
    */
    
    Ok(())
  }

    pub fn create_collection<'a, 'b, 'c, 'info>(ctx: Context<'a, 'b, 'c, 'info, CreateCollection<'info>>, token_metadata:crate::cnft::TokenMetadata, vault_type:String) -> Result<()> {
      
      
      let token_account_info = ctx.accounts.token_account.to_account_info();
      
      let mint_info = ctx.accounts.mint.to_account_info();
      let metadata_info = ctx.accounts.metadata.to_account_info();
      
      
      let creator_authority_info = ctx.accounts.creator_authority.to_account_info();
      
      let store_key = ctx.accounts.store.key();
      
      
      let payer_info = ctx.accounts.payer.to_account_info();
      let rent_info = ctx.accounts.rent.to_account_info();
      
      let payer_key = payer_info.key();
      
      let symbol_lw = token_metadata.symbol.to_lowercase();
      
      let mint_bump = &ctx.bumps.mint.to_le_bytes();
      let firma:&[&[u8]] = &[
        vault_type.as_ref(),
        store_key.as_ref(),
        mint_bump
      ];
      
      
      
      let creator_authority_bump = &ctx.bumps.creator_authority.to_le_bytes();
      let firma_creator:&[&[u8]] = &[
        b"tree".as_ref(),
        creator_authority_bump
      ];
      
      
      let uri = match token_metadata.asset_bundler {
            AssetBundler::Arweave => {
                "https://arweave.net/"
            }
            AssetBundler::IrysGateway => {
                "https://gateway.irys.xyz/"
            }
            _ => {
                "https://arweave.net/"
            }
        }.to_string() + token_metadata.arweave.as_str();

      
      let signature = &[firma, firma_creator];
      
      let create_result = create_metadata_v3(
        &metadata_info,
        &mint_info,
        &mint_info,
        &payer_info,
        &creator_authority_info,
        &rent_info,
        &ctx.accounts.system_program.to_account_info(),
        token_metadata.name.clone(),
        token_metadata.symbol.clone(),
        uri,
        0,
        Some(true),
        signature, 
        ctx.remaining_accounts);
      
      match create_result {
        Ok(()) => {
        }
        Err(err) => {
          return Err(err)
        }
      }
      
      let mint_accounts = MintTo {
        mint:mint_info.clone(),
        to:token_account_info.clone(),
        authority:mint_info.clone(),
      };
      
      
       let inner = firma.to_vec(); 
       let inner2 = firma_creator.to_vec();
        
        let outer = &[inner.as_slice()];
      
      
      let keyp = ctx.accounts.token_program.key();
      let cpi_program = ctx.accounts.token_program.to_account_info();
      let owner = cpi_program.owner;
      
              msg!("mint porgram {:?}",keyp);
      let cpi_ctx = CpiContext::new_with_signer(cpi_program, mint_accounts, outer).with_remaining_accounts(vec![]);
      let good = mint_to(cpi_ctx, 1);
      
      match good {
          Ok(()) => {
              msg!("good mint");
          }
          Err(err) => {
              msg!("bad mint");
            return Err(err)
          }
        }
        
      
      
      
      let edition_signature = &[firma, firma_creator];
      
      let edition_info = ctx.accounts.edition.to_account_info();
      
      let create_master_result = create_master_edition_v3(
        &edition_info,
        &ctx.accounts.mint.to_account_info(),
        &creator_authority_info,
        &ctx.accounts.mint.to_account_info(),
        &metadata_info,
        &payer_info,
        &rent_info,
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.token_program.to_account_info(),
        0,
        edition_signature, 
        ctx.remaining_accounts);
      
      match create_master_result {
        Ok(()) => {
        }
        Err(err) => {
          return Err(err)
        }
      }
      
      
      
      match good {
        Ok(()) => {
          msg!("g");
        }
        Err(_) => {
           return Err(ProgramError::InvalidArgument.into())
         }
      }
      
      Ok(())
    }
    


    pub fn create_store(ctx: Context<CreateStore>,slot:u16) -> Result<()> {
    
        let universe = &mut ctx.accounts.universe;
        let store = &mut ctx.accounts.store;
        let master = &mut ctx.accounts.master;
        
        universe.count += 1;
        
        
        let store_key = store.key();
        let master_key = master.key();
        
        let store_hash = cyrb53_bytes(&store_key.to_bytes(),0);
        
        let creator = ctx.accounts.creator.key();
        
        store.class = AccountClass::StoreV1;
        store.creator = creator;
        store.slot = slot;
        store.universe = universe.key();
        store.store_hash = store_hash;
        store.master = master_key;
        store.master_manager = master.manager;
        
        Ok(())
      }

      pub fn create_master(ctx: Context<CreateMaster>, slot:u16) -> Result<()> {
    
        // let map = &mut ctx.accounts.map;
       
       let master = &mut ctx.accounts.master;
       let manager = &mut ctx.accounts.manager;
       
       //map.master = master.key();
       
       let universe = &mut ctx.accounts.universe;
       
       let universe_key = universe.key();
       let universe_hash = cyrb53_bytes(&universe_key.to_bytes(),0);
       
       master.class = AccountClass::MasterV1;
       master.manager = manager.key();
       master.slot = slot;
       master.universe_hash = universe_hash;
       
       Ok(())
     }

    pub fn create_universe(ctx: Context<CreateUniverse>,slot:u16) -> Result<()> {
    
        let universe = &mut ctx.accounts.universe;
        
        let universe_key = universe.key();
        
        let universe_hash = cyrb53_bytes(&universe_key.to_bytes(),0);
        
        let creator = ctx.accounts.creator.key();
        
        universe.class = AccountClass::UniverseV1;
        universe.creator = creator;
        universe.slot = slot;
        universe.universe_hash = universe_hash;
        
        Ok(())
      }
}



#[derive(Accounts)]
#[instruction(slot:u16)]
pub struct CreateMaster<'info> {
  #[account(
  init,
  seeds = [b"master".as_ref(), universe.key().as_ref(), manager.key().as_ref(), &slot.to_le_bytes().as_ref()],
  bump,
  payer = creator,
  space = 8+75
  )]
  pub master: Box<Account<'info, Master>>,
  #[account(mut)]
  /// CHECK: This is just used as a signing PDA.
  pub manager: UncheckedAccount<'info>,
  pub universe: Box<Account<'info, Universe>>,
 // pub map: Account<'info, Map>,
  #[account(mut)]
  pub creator: Signer<'info>,
  pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
#[instruction(slot:u16)]
pub struct CreateStore<'info> {
  #[account(
    init,
    seeds = [b"store".as_ref(), universe.key().as_ref(), creator.key().as_ref(), &slot.to_le_bytes().as_ref()],
    bump,
    payer = creator,
    space = 8+MIN_STORE
    )]
    pub store: Box<Account<'info, Store>>,
    /// CHECK
    pub master: Box<Account<'info, Master>>,
    #[account(mut)]
    pub universe: Box<Account<'info, Universe>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
#[instruction(slot:u16)]
pub struct CreateUniverse<'info> {
  #[account(
  init,
  seeds = [b"universe".as_ref(), creator.key().as_ref(), &slot.to_le_bytes().as_ref()],
  bump,
  payer = creator,
  space = 8+239
  )]
  pub universe: Account<'info, Universe>,
  #[account(mut)]
  pub creator: Signer<'info>,
  pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
#[instruction(max_depth:u32, max_buffer_size:u32, public:bool)]
pub struct FeedGlobalTree<'info> {
   #[account(mut, owner = Pubkey::new_from_array(SPL_ACCOUNT_COMPRESSION))]
  /// CHECK: This account must be all zeros
  pub merkle_account: UncheckedAccount<'info>,
  #[account(
  init_if_needed,
  seeds = [b"tree".as_ref()],
  bump,
  payer = payer,
  space = 0
  )]
  /// CHECK: unsafe
  pub merkle_manager: UncheckedAccount<'info>, 
  #[account(mut)]
  /// CHECK: This is just used as a signing PDA.
  pub tree_authority: UncheckedAccount<'info>,
  pub log_wrapper: Program<'info, Noop>,
  pub bubblegum_program: Program<'info, Bubblegum>,
  pub compression_program: Program<'info, SplAccountCompression>,
  #[account(mut)]
  pub payer: Signer<'info>,
  pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(token_metadata:crate::cnft::TokenMetadata, vault_type:String)]
pub struct CreateCollection<'info> {
  /// CHECK: unsafe
  #[account(
      mut,
      seeds = [b"tree".as_ref()],
      bump
      )]
  pub creator_authority: UncheckedAccount<'info>,
  #[account(mut)]
  /// CHECK: unsafe
  pub metadata: UncheckedAccount<'info>,
  #[account(mut)]
  /// CHECK: unsafe
  pub edition: UncheckedAccount<'info>,
  #[account(
      init,
      seeds = [vault_type.as_ref(), store.key().as_ref()],
      bump,
      payer = payer,
      mint::decimals = 0,
      mint::authority = mint,
      mint::freeze_authority = mint,
  )]
  pub mint: Box<InterfaceAccount<'info, Mint>>,
  #[account(
      init,
      payer = payer, 
      associated_token::mint = mint, 
      associated_token::authority = creator_authority,
      associated_token::token_program = token_program
  )]
  pub token_account: Box<InterfaceAccount<'info, TokenAccount>>,
  #[account(
   mut,
     seeds = [b"store".as_ref(), store.universe.as_ref(), payer.key().as_ref(), &store.slot.to_le_bytes().as_ref()],
     bump
    )]
  pub store: Box<Account<'info, Store>>,
  #[account(mut)]
  pub payer: Signer<'info>,
  pub token_program: Program<'info, Token>,
  pub token_metadata_program: Program<'info, MplTokenMetadata>,
  pub associated_token_program: Program<'info, AssociatedToken>,
  /// CHECK: unsafe
  pub rent: UncheckedAccount<'info>,
  pub system_program: Program<'info, System>,
}



#[account]
//239
pub struct Universe {
  pub class:AccountClass, //1
  pub slot: u16, //2
  pub creator: Pubkey, //32
  pub universe_hash:u64, //8
  pub count:u32, //4
  pub extra:[u8; 192] //192
}

#[account]
//299
pub struct Store {
  pub class:AccountClass, //1
  pub creator: Pubkey, //32
  pub universe:Pubkey, //32
  pub slot: u16, //2
  pub store_hash:u64, //8
  pub master: Pubkey, //32
  pub master_manager: Pubkey, //32
  pub living_campaigns:u64, //8
  pub living_spots:u64, //8
  pub extra:[u8; 144] //1$4
}

#[account]
//75
pub struct Master {
  pub class:AccountClass, //1
  pub slot: u16, //2
  pub universe_hash:u64, //8
  pub manager: Pubkey, //32
  pub extra:[u8; 32] //20
}


  pub const MIN_STORE:usize = 299;

 

