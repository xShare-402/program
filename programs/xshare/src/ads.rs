use anchor_lang::prelude::*;
use crate::AccountClass;
use crate::base::{MplTokenMetadata,Store};


const AD_BID_TRACK_DISCRIMINATOR:[u8;8] = [58, 182, 194, 237, 251,  44, 148,  79];



pub const DEVNET_VALIDATOR:[u8;32] = [5, 62, 162, 42, 88, 56, 179, 161, 33, 20, 15, 2, 170, 67, 59, 11, 147, 146, 74, 122, 186, 28, 135, 70, 6, 17, 193, 187, 246, 101, 67, 254];

use crate::bloom::optimal_bloom_parameters;

use anchor_lang::system_program::{create_account, CreateAccount};
use crate::utils::{bytes_to_string,cyrb53_bytes,encode_bytes_to_ascii_string};
use mpl_bubblegum::utils::get_asset_id;

use crate::cnft::{mint_to_collection_cnft,burn_cnft,MetadataArgs,AssetBundler,Creator,SplAccountCompression, Bubblegum, Collection, TokenStandard, TokenProgramVersion,Noop};
use crate::utils::{encode_u64_to_ascii_string, verify_signature,hash_to_u8_array};

use ephemeral_rollups_sdk::{commit_record_seeds_from_delegated_account,commit_state_seeds_from_delegated_account};
use ephemeral_rollups_sdk::anchor::{commit, delegate, ephemeral};


use ephemeral_rollups_sdk::ephem::{commit_accounts, commit_and_undelegate_accounts};
use ephemeral_rollups_sdk::cpi::{DelegateConfig,DelegateAccounts, delegate_account, undelegate_account};

use crate::adscategories::AdCategory;


pub mod ads_ix {

    use super::*;

    pub fn register_ad_creator(ctx: Context<RegisterAdCreator>) -> Result<()> {
        
        let ad_creator = &mut ctx.accounts.ad_creator;
        let creator = ctx.accounts.creator.key();

        {
            let ad_creator_manager = &mut ctx.accounts.ad_creator_manager;
            if let Some(ad_creator_manager) = ad_creator_manager {
                let ad_creator_manager_info = ad_creator_manager.to_account_info();
                let lamports = ad_creator_manager_info.lamports();

                let rent = Rent::get()?;
                let min_base = rent.minimum_balance(0) + (5000 + 10000)*2;

                if min_base > lamports {
                    let needs_at_least = min_base - lamports;

                    let ix = anchor_lang::solana_program::system_instruction::transfer( &creator, &ad_creator_manager_info.key(),  needs_at_least); 
                    anchor_lang::solana_program::program::invoke( &ix, &[ ctx.accounts.creator.to_account_info(), ad_creator_manager_info ])?;

                }
                ad_creator.creator_manager = ad_creator_manager.key();
            }
        }

        let store = &ctx.accounts.store;
        let store_key = store.key();
        let store_hash = cyrb53_bytes(&store_key.to_bytes(),0);
        
        
        
        ad_creator.class = AccountClass::AdCreatorV1;
        ad_creator.creator = creator;
        ad_creator.store_hash = store_hash;
        
        
        Ok(())
      }

      pub fn register_space_creator(ctx: Context<RegisterSpaceCreator>) -> Result<()> {
        
        let space_creator = &mut ctx.accounts.space_creator;


        let store = &ctx.accounts.store;
        let store_key = store.key();
        let store_hash = cyrb53_bytes(&store_key.to_bytes(),0);
        
        let creator = ctx.accounts.creator.key();
        
        space_creator.class = AccountClass::SpaceCreatorV1;
        space_creator.creator = creator;
        space_creator.store_hash = store_hash;
        space_creator.creator_manager = ctx.accounts.ad_creator_manager.key();
        
        Ok(())
      }

      pub fn delete_ad_spot<'a, 'b, 'c, 'info>(ctx: Context<'a, 'b, 'c, 'info, DeleteAdSpot<'info>>, cnft: CampaignCnft) -> Result<()> {
        let ad_spot = &mut ctx.accounts.ad_spot;
        let ad_spot_info = ad_spot.to_account_info();

        {
            if ad_spot.live_bids > 0 {
                msg!("close its bids first");
                return Err(ProgramError::InvalidArgument.into())
            }
        }
        
        let creator = &ctx.accounts.creator;
        let creator_key = creator.key();

        let store = &mut ctx.accounts.store;
        let store_key = store.key();

        let ad_creator = &mut ctx.accounts.ad_creator;
        let ad_creator_key = ad_creator.key();
        
        let bubblegum_program_info = ctx.accounts.bubblegum_program.to_account_info();
        let tree_authority_info = ctx.accounts.tree_authority.to_account_info();
        let merkle_tree_info = ctx.accounts.merkle_tree.to_account_info();
        let compression_program_info = ctx.accounts.compression_program.to_account_info();
        let log_wrapper_info = ctx.accounts.log_wrapper.to_account_info();
        let system_program_info = ctx.accounts.system_program.to_account_info();
        
        let nonce = cnft.index as u64;
        let leaf_index = cnft.index;

        let ad_creator_info = ad_creator.to_account_info();


        let mut creators = vec![];


        creators.push(Creator {
            address:creator_key.clone(),
            share:100,
            verified:false
        });
        
        creators.push(Creator {
            address:ad_creator_key.clone(),
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:ad_spot_info.key(),
            share:0,
            verified:false
        });

        let store_hash_bytes = store.store_hash.to_le_bytes();
        let ad_creator_bump = &ctx.bumps.ad_creator.to_le_bytes();    
        let space_creator_seeds = vec![
            b"ad_creator".as_ref(),
            creator_key.as_ref(),
            &store_hash_bytes.as_ref(),
            ad_creator_bump
        ];

        let creator_data = creators.iter().map(|c| { Ok([c.address.as_ref(), &[c.verified as u8], &[c.share]].concat()) }).collect::<Result<Vec<_>>>()?;
        let creator_hash = solana_program::keccak::hashv( creator_data.iter().map(|c| c.as_slice()).collect::<Vec<&[u8]>>().as_ref());

        let result = burn_cnft(
            &tree_authority_info,
            &ad_creator_info,
            &ad_creator_info,
            &merkle_tree_info,
            &log_wrapper_info,
            &compression_program_info,
            &system_program_info,
            &bubblegum_program_info,
            cnft.root,
            cnft.data_hash,
            hash_to_u8_array(creator_hash),
            nonce,
            leaf_index,
            space_creator_seeds,
            ctx.remaining_accounts
        );

        ad_creator.living_spots -= 1;
        store.living_spots -= 1;
        
        match result {
        Ok(()) => {
        }
        Err(err) => {
            // Handle error case  
            return Err(err)
        }
        } 
        Ok(())


      }

      pub fn delete_ad_campaign<'a, 'b, 'c, 'info>(ctx: Context<'a, 'b, 'c, 'info, DeleteAdCampaign<'info>>, cnft: CampaignCnft) -> Result<()> {
        let ad_campaign = &mut ctx.accounts.ad_campaign;
        let ad_campaign_info = ad_campaign.to_account_info();
        
        let creator = &ctx.accounts.creator;
        let creator_key = creator.key();

        let store = &mut ctx.accounts.store;
        let store_key = store.key();

        let space_creator = &mut ctx.accounts.space_creator;
        let space_creator_key = space_creator.key();
        
        let bubblegum_program_info = ctx.accounts.bubblegum_program.to_account_info();
        let tree_authority_info = ctx.accounts.tree_authority.to_account_info();
        let merkle_tree_info = ctx.accounts.merkle_tree.to_account_info();
        let compression_program_info = ctx.accounts.compression_program.to_account_info();
        let log_wrapper_info = ctx.accounts.log_wrapper.to_account_info();
        let system_program_info = ctx.accounts.system_program.to_account_info();
        
        let nonce = cnft.index as u64;
        let leaf_index = cnft.index;

        let space_creator_info = space_creator.to_account_info();

        let twitter:[u8;32] = {
            let ref_data = ad_campaign_info.try_borrow_data()?;
            ref_data[TO_AD_CAMPAIGN_TWITTER..TO_AD_CAMPAIGN_TWITTER+32].try_into().unwrap()
        };

        let mut creators = vec![];

        let (twitter_key,bump) = Pubkey::find_program_address(&[b"twitter", store_key.as_ref(), twitter.as_ref()], &crate::ID);

        creators.push(Creator {
            address:creator_key.clone(),
            share:100,
            verified:false
        });
        
        creators.push(Creator {
            address:space_creator_key.clone(),
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:ad_campaign_info.key(),
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:twitter_key,
            share:0,
            verified:false
        });

        let store_hash_bytes = store.store_hash.to_le_bytes();
        let space_creator_bump = &ctx.bumps.space_creator.to_le_bytes();    
        let space_creator_seeds = vec![
            b"space_creator".as_ref(),
            creator_key.as_ref(),
            &store_hash_bytes.as_ref(),
            space_creator_bump
        ];

        let creator_data = creators.iter().map(|c| { Ok([c.address.as_ref(), &[c.verified as u8], &[c.share]].concat()) }).collect::<Result<Vec<_>>>()?;
        let creator_hash = solana_program::keccak::hashv( creator_data.iter().map(|c| c.as_slice()).collect::<Vec<&[u8]>>().as_ref());

        let result = burn_cnft(
            &tree_authority_info,
            &space_creator_info,
            &space_creator_info,
            &merkle_tree_info,
            &log_wrapper_info,
            &compression_program_info,
            &system_program_info,
            &bubblegum_program_info,
            cnft.root,
            cnft.data_hash,
            hash_to_u8_array(creator_hash),
            nonce,
            leaf_index,
            space_creator_seeds,
            ctx.remaining_accounts
        );

        space_creator.living -= 1;
        store.living_campaigns -= 1;
        
        match result {
        Ok(()) => {
        }
        Err(err) => {
            // Handle error case  
            return Err(err)
        }
        } 
        Ok(())


      }

       /* pub fn grow_ad_bids(ctx: Context<GrowAdBids>) -> Result<()> {
        

        let ad_campaign = &mut ctx.accounts.ad_campaign;
        let ad_track = &mut ctx.accounts.ad_track;

        {
            let ad_track_info = ad_track.to_account_info();
            let mut ref_data = ad_track_info.try_borrow_mut_data()?;

            let (bloom_bits, k) = optimal_bloom_parameters( ad_campaign.config.max_bidders as usize * 10, 1.0 / ad_campaign.config.bloom_accuracy as f64);
            let bytes_in_bloom:usize = (bloom_bits / 8) * 2;

            let max_space = 10240 - START_FROM_AD_BIDS + 8;
            if bytes_in_bloom <= max_space {
                ref_data[9] = 1;
                ref_data[10] = 1;
            }

        }

            Ok(())

      } */

      
      pub fn register_ad_spot(ctx: Context<RegisterAdSpot>, slot:u32, asset_source:AssetSource) -> Result<()> {
        let ad_spot = &mut ctx.accounts.ad_spot;

        let store = &mut ctx.accounts.store;
        let store_key = store.key();
        let store_hash = cyrb53_bytes(&store_key.to_bytes(),0);
        
        let creator = &ctx.accounts.creator;
        let creator_key = creator.key();

        ad_spot.class = AccountClass::AdSpotV1;
        ad_spot.creator = creator_key;
        ad_spot.store_hash = store_hash;
        ad_spot.slot = slot;

        let ad_creator = &mut ctx.accounts.ad_creator;
        let ad_creator_key = ad_creator.key();

        let ad_spot_info = ad_spot.to_account_info();

        let mut creators = vec![];


        creators.push(Creator {
            address:creator_key.clone(),
            share:100,
            verified:false
        });
        
        creators.push(Creator {
            address:ad_creator_key.clone(),
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:ad_spot_info.key(),
            share:0,
            verified:false
        });

        

        let clock = Clock::get().unwrap();
        let clock32 = clock.unix_timestamp.clamp(0, u32::MAX as i64) as u32;


        let mut payload:Vec<u8> = vec![];
        payload.push(0);
        payload.extend(clock32.to_le_bytes());

        let mut uri = match asset_source.asset_bundler {
            AssetBundler::Arweave => {
                "https://arweave.net/"
            }
            AssetBundler::IrysGateway => {
                "https://gateway.irys.xyz/"
            }
            AssetBundler::Raw => {
                "https://"
            }
            _ => {
                "https://arweave.net/"
            }
        }.to_string() + asset_source.arweave.as_str();
        
        if payload.len() > 0 {
            uri += "?p=";
            uri += encode_bytes_to_ascii_string(&payload, true).as_str();
        }

        let metadata = MetadataArgs {
            name:"Space V0".to_string(),
            symbol:encode_u64_to_ascii_string(ad_creator.created_spots as u64),
            uri,
            seller_fee_basis_points: 0,
            creators,
            primary_sale_happened: false,
            is_mutable: true,
            edition_nonce: None,
            collection: Some(Collection { verified:true, key:ctx.accounts.collection_mint.key() }),
            uses: None,
            token_standard: Some(TokenStandard::NonFungible),
            token_program_version: TokenProgramVersion::Original,
        };

        let merkle_manager_bump = &ctx.bumps.merkle_manager.to_le_bytes();
        let merkle_manager = &[
            b"tree".as_ref(),
            merkle_manager_bump
        ];

        let signature:Vec<&[&[u8]]> = vec![
            merkle_manager
        ];
        
        let bubblegum_program_info = ctx.accounts.bubblegum_program.to_account_info();
        let tree_authority_info = ctx.accounts.tree_authority.to_account_info();
        let merkle_tree_info = ctx.accounts.merkle_tree.to_account_info();
        let compression_program_info = ctx.accounts.compression_program.to_account_info();
        let log_wrapper_info = ctx.accounts.log_wrapper.to_account_info();
        let merkle_manager_info = ctx.accounts.merkle_manager.to_account_info();
        
        
        let edition_account_info = ctx.accounts.edition_account.to_account_info();
        let collection_mint_info = ctx.accounts.collection_mint.to_account_info();
        let bubblegum_signer_info = ctx.accounts.bubblegum_signer.to_account_info();
        let collection_metadata_info = ctx.accounts.collection_metadata.to_account_info();
        let token_metadata_program_info = ctx.accounts.token_metadata_program.to_account_info();
        let system_program_info = ctx.accounts.system_program.to_account_info();


        let ad_creator_info = ad_creator.to_account_info();
        let creator_info = creator.to_account_info();

        let result = mint_to_collection_cnft(
        &bubblegum_program_info,
        &tree_authority_info,
        &ad_creator_info,
        &ad_creator_info,
        &merkle_tree_info,
        &creator_info,
        &merkle_manager_info,
        &merkle_manager_info,
        &bubblegum_program_info,
        &collection_mint_info,
        &collection_metadata_info,
        &edition_account_info,
        &bubblegum_signer_info,
        &log_wrapper_info,
        &compression_program_info,
        &token_metadata_program_info,
        &system_program_info,
        metadata,
        &signature,
        &[]);
        
        match result {
            Ok(()) => {},
            Err(err) => return Err(err),
        }
        
        let tree_data = tree_authority_info.try_borrow_data()?;
        let num_minted = u64::from_le_bytes(tree_data[80..88].try_into().unwrap());
        let asset_id = get_asset_id(&ctx.accounts.merkle_tree.key(), num_minted - 1);

        ad_creator.living_spots += 1;
        store.living_spots += 1;
        ad_creator.created_spots += 1;


        Ok(())

      }

      pub fn prepare_delete_ad_campaign(ctx: Context<PrepareDeleteAdCampaign>) -> Result<()> {

        let space_creator = &ctx.accounts.space_creator;

        let store = &ctx.accounts.store;
        let creator_key = ctx.accounts.creator.key();

        let manager = store.master_manager;

        if creator_key != manager && space_creator.creator != creator_key && space_creator.creator_manager != creator_key {
            return Err(ProgramError::InvalidAccountOwner.into())
        }

        {
            let ad_campaign_info = ctx.accounts.ad_campaign.to_account_info();
            let ref_data = ad_campaign_info.try_borrow_data()?;
            
            let living_bids = u32::from_le_bytes(ref_data[TO_AD_CAMPAIGN_LIVING_BID..TO_AD_CAMPAIGN_LIVING_BID+4].try_into().unwrap());

            if living_bids > 0 {
                msg!("to close bids first");
                return Err(ProgramError::InvalidInstructionData.into())
            }
        }

        //let ad_campaign = &ctx.accounts.ad_campaign;

        let ad_campaign_bid_tracks = &mut ctx.accounts.ad_campaign_bid_tracks;

        msg!("good {:?}", crate::ID);

        let ad_campaign_bid_tracks_info = ad_campaign_bid_tracks.to_account_info();

        ad_campaign_bid_tracks_info.resize(START_FROM_AD_CAMPAIGN_BID_TRACKS)?;

        {
            let mut ref_data = ad_campaign_bid_tracks_info.try_borrow_mut_data()?;
            ref_data[TO_AD_CAMPAIGN_BID_TRACK_DELEGATION_STATE] = AD_CAMPAIGN_BID_TRACK_DELETING_STATE;
        }



        let good = commit_and_undelegate_accounts(
            &ctx.accounts.creator.to_account_info(),
            vec![&ad_campaign_bid_tracks_info],
            &ctx.accounts.magic_context,
            &ctx.accounts.magic_program,
        );

        match good {
            Ok(()) => {
            }
            Err(err) => {
                // Handle error case  
                msg!("err {:?}",err);
                return Err(ProgramError::InvalidAccountOwner.into())
            }
        } 


        Ok(())
      }
     
      pub fn register_ad_campaign(ctx: Context<RegisterAdCampaign>, twitter_proof:[u8;64], slot:u32, campaign_config:CampaignConfig, twitter:[u8;32], asset_source:AssetSource) -> Result<()> {
        

        let ad_campaign = &mut ctx.accounts.ad_campaign;
        let ad_campaign_key = ad_campaign.key();

        let store = &mut ctx.accounts.store;
        let store_key = store.key();
        let store_hash = cyrb53_bytes(&store_key.to_bytes(),0);
        
        let creator = &ctx.accounts.creator;
        let creator_key = creator.key();

        {

            let ad_campaign_info = ad_campaign.to_account_info();
            let mut ref_data = ad_campaign_info.try_borrow_mut_data()?;


            ref_data[8] = AccountClass::AdCampaignV1 as u8;
            ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].copy_from_slice(&creator_key.to_bytes());
            ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].copy_from_slice(&store_hash.to_le_bytes());
            ref_data[TO_AD_CAMPAIGN_SLOT..TO_AD_CAMPAIGN_SLOT+4].copy_from_slice(&slot.to_le_bytes());
            ref_data[TO_AD_CAMPAIGN_TWITTER..TO_AD_CAMPAIGN_TWITTER+32].copy_from_slice(&twitter.as_ref());
            ref_data[TO_AD_CAMPAIGN_CONFIG..TO_AD_CAMPAIGN_CONFIG+AD_CAMPAIGN_CONFIG_SIZE].copy_from_slice(&campaign_config.try_to_vec()?);

        }


        {

            let ad_campaign_bid_tracks = &mut ctx.accounts.ad_campaign_bid_tracks;
            let ad_campaign_bid_tracks_info = ad_campaign_bid_tracks.to_account_info();
            let system_program_info = ctx.accounts.system_program.to_account_info();
            let creator_info = ctx.accounts.creator.to_account_info();

            let ad_campaign_bid_tracks_bump = ctx.bumps.ad_campaign_bid_tracks.to_le_bytes();

            let bytes = START_FROM_AD_CAMPAIGN_BID_TRACKS + SPACE_IN_CAMPAIGN_BID_TRACK;
            let rent = Rent::get()?;
            let min_base = rent.minimum_balance(bytes);
            
            let created = create_account(
                CpiContext::new_with_signer(
                system_program_info.clone(),
                CreateAccount {
                    from: creator_info.clone(),
                    to: ad_campaign_bid_tracks_info.clone()
                },
                &[&[
                    b"ad_campaign_bid_tracks".as_ref(),
                    ad_campaign_key.as_ref(),
                    &ad_campaign_bid_tracks_bump
                ]]
                ),
                min_base,
                bytes as u64,
                &crate::ID
            );
            match created {
                    Ok(()) => {
                    }
                    Err(err) => {
                        return Err(ProgramError::InvalidAccountOwner.into())
                    }
            }
            
            
            let mut ref_data = ad_campaign_bid_tracks_info.try_borrow_mut_data()?;

            ref_data[0..8].copy_from_slice(&AD_BID_TRACK_DISCRIMINATOR);
            ref_data[8] = AccountClass::AdCampaignBidTrackV1 as u8;
            ref_data[TO_AD_CAMPAIGN_BID_TRACK_CREATOR..TO_AD_CAMPAIGN_BID_TRACK_CREATOR+32].copy_from_slice(&creator_key.to_bytes());
            ref_data[TO_AD_CAMPAIGN_BID_TRACK_STORE_HASH..TO_AD_CAMPAIGN_BID_TRACK_STORE_HASH+8].copy_from_slice(&store_hash.to_le_bytes());
            ref_data[TO_AD_CAMPAIGN_BID_TRACK_CAMPAIGN..TO_AD_CAMPAIGN_BID_TRACK_CAMPAIGN+32].copy_from_slice(&ad_campaign_key.to_bytes());
            
            ref_data[TO_AD_CAMPAIGN_BID_TRACK_DELEGATION_STATE] = AD_CAMPAIGN_BID_TRACK_DELEGATING_STATE;

        }

        


        let mut message:[u8;64] = [0;64];
        message[0..32].copy_from_slice(&creator_key.to_bytes());
        message[32..64].copy_from_slice(&twitter);
        
        

        let verified = verify_signature(&message, &twitter_proof, &store.master_manager.to_bytes());

        match verified {
            Ok(())=>{

            }
            Err(_) => {
                return Err(ProgramError::InvalidAccountOwner.into())
            }
        }

        let space_creator = &mut ctx.accounts.space_creator;
        let space_creator_key = space_creator.key();

        let ad_campaign_info = ad_campaign.to_account_info();

        let mut creators = vec![];

        let (twitter_key,bump) = Pubkey::find_program_address(&[b"twitter", store_key.as_ref(), twitter.as_ref()], &crate::ID);

        creators.push(Creator {
            address:creator_key.clone(),
            share:100,
            verified:false
        });
        
        creators.push(Creator {
            address:space_creator_key.clone(),
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:ad_campaign_info.key(),
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:twitter_key,
            share:0,
            verified:false
        });

        

        let clock = Clock::get().unwrap();
        let clock32 = clock.unix_timestamp.clamp(0, u32::MAX as i64) as u32;

        let cut_twitter:Vec<u8> = twitter.to_vec().into_iter().filter(|b| b != &0).collect();

        let mut payload:Vec<u8> = vec![];
        payload.push(0);
        payload.extend(clock32.to_le_bytes());
        payload.push(campaign_config.permission as u8);
        payload.push(campaign_config.max_seconds as u8);
        payload.push(cut_twitter.len() as u8);
        payload.extend(cut_twitter);

        let mut uri = match asset_source.asset_bundler {
            AssetBundler::Arweave => {
                "https://arweave.net/"
            }
            AssetBundler::IrysGateway => {
                "https://gateway.irys.xyz/"
            }
            AssetBundler::Raw => {
                "https://"
            }
            _ => {
                "https://arweave.net/"
            }
        }.to_string() + asset_source.arweave.as_str();
        
        if payload.len() > 0 {
            uri += "?p=";
            uri += encode_bytes_to_ascii_string(&payload, true).as_str();
        }

        let metadata = MetadataArgs {
            name:"Space V0".to_string(),
            symbol:encode_u64_to_ascii_string(space_creator.created as u64),
            uri,
            seller_fee_basis_points: 0,
            creators,
            primary_sale_happened: false,
            is_mutable: true,
            edition_nonce: None,
            collection: Some(Collection { verified:true, key:ctx.accounts.collection_mint.key() }),
            uses: None,
            token_standard: Some(TokenStandard::NonFungible),
            token_program_version: TokenProgramVersion::Original,
        };

        let merkle_manager_bump = &ctx.bumps.merkle_manager.to_le_bytes();
        let merkle_manager = &[
            b"tree".as_ref(),
            merkle_manager_bump
        ];

        let signature:Vec<&[&[u8]]> = vec![
            merkle_manager
        ];
        
        let bubblegum_program_info = ctx.accounts.bubblegum_program.to_account_info();
        let tree_authority_info = ctx.accounts.tree_authority.to_account_info();
        let merkle_tree_info = ctx.accounts.merkle_tree.to_account_info();
        let compression_program_info = ctx.accounts.compression_program.to_account_info();
        let log_wrapper_info = ctx.accounts.log_wrapper.to_account_info();
        let merkle_manager_info = ctx.accounts.merkle_manager.to_account_info();
        
        
        let edition_account_info = ctx.accounts.edition_account.to_account_info();
        let collection_mint_info = ctx.accounts.collection_mint.to_account_info();
        let bubblegum_signer_info = ctx.accounts.bubblegum_signer.to_account_info();
        let collection_metadata_info = ctx.accounts.collection_metadata.to_account_info();
        let token_metadata_program_info = ctx.accounts.token_metadata_program.to_account_info();
        let system_program_info = ctx.accounts.system_program.to_account_info();


        let space_creator_info = space_creator.to_account_info();
        let creator_info = creator.to_account_info();

        let result = mint_to_collection_cnft(
        &bubblegum_program_info,
        &tree_authority_info,
        &space_creator_info,
        &space_creator_info,
        &merkle_tree_info,
        &creator_info,
        &merkle_manager_info,
        &merkle_manager_info,
        &bubblegum_program_info,
        &collection_mint_info,
        &collection_metadata_info,
        &edition_account_info,
        &bubblegum_signer_info,
        &log_wrapper_info,
        &compression_program_info,
        &token_metadata_program_info,
        &system_program_info,
        metadata,
        &signature,
        &[]);
        
        match result {
            Ok(()) => {},
            Err(err) => return Err(err),
        }
        
        let tree_data = tree_authority_info.try_borrow_data()?;
        let num_minted = u64::from_le_bytes(tree_data[80..88].try_into().unwrap());
        let asset_id = get_asset_id(&ctx.accounts.merkle_tree.key(), num_minted - 1);

        space_creator.living += 1;
        store.living_campaigns += 1;
        space_creator.created += 1;


        {

            let seed_1 = b"ad_campaign_bid_tracks";
            let seed_2 = &ad_campaign_key.to_bytes();
            let pda_seeds: &[&[u8]] = &[seed_1, seed_2];

            ctx.accounts.delegate_ad_campaign_bid_tracks(
                &ctx.accounts.creator,
                pda_seeds,
                DelegateConfig {
                    // Optionally set a specific validator from the first remaining account
                    validator: Some(Pubkey::new_from_array(DEVNET_VALIDATOR)),
                    ..Default::default()
                },
            )?;

           /* let good = delegate_account(
                &ctx.accounts.creator.to_account_info(),
                vec![&ad_campaign_bid_tracks_info],
                &ctx.accounts.magic_context,
                &ctx.accounts.magic_program,
            );

            match good {
                Ok(()) => {
                }
                Err(err) => {
                    // Handle error case  
                    msg!("err {:?}",err);
                    return Err(ProgramError::InvalidAccountOwner.into())
                }
            } 
*/
        }


        /*{

            let ad_campaign_bid_tracks = &ctx.accounts.ad_campaign_bid_tracks;
            let ad_campaign_bid_tracks_info = ad_campaign_bid_tracks.to_account_info();
            
            let system_program = ctx.accounts.system_program.to_account_info();
            let delegation_program = ctx.accounts.delegation_program.to_account_info();
            let delegation_metadata = ctx.accounts.delegation_metadata.to_account_info();
            let delegation_record = ctx.accounts.delegation_record.to_account_info();
            let delegation_buffer = ctx.accounts.delegation_buffer.to_account_info();

            let delegate_accounts = DelegateAccounts {
                payer: &ctx.accounts.creator.to_account_info(),
                pda: &ad_campaign_bid_tracks_info,
                owner_program: &ctx.accounts.own_program.to_account_info(),
                buffer: &delegation_buffer,
                delegation_record:&delegation_record,
                delegation_metadata:&delegation_metadata,
                delegation_program:&delegation_program,
                system_program:&system_program,
            };

            let seed_1 = b"ad_campaign_bid_tracks";
            let seed_2 = &ad_campaign_key.to_bytes();
            let pda_seeds: &[&[u8]] = &[seed_1, seed_2];

            let delegate_config = DelegateConfig {
                commit_frequency_ms: 0,
                validator: Some(Pubkey::new_from_array(DEVNET_VALIDATOR)),
            };

            delegate_account(delegate_accounts, pda_seeds, delegate_config)?;


         }*/



        
        Ok(())
      }

      pub fn register_ad_space(ctx: Context<RegisterAdSpace>, slot:u32) -> Result<()> {
        
        let ad_space = &mut ctx.accounts.ad_space;


        let store = &ctx.accounts.store;
        let store_key = store.key();
        let store_hash = cyrb53_bytes(&store_key.to_bytes(),0);
        
        let creator = ctx.accounts.creator.key();
        
        ad_space.class = AccountClass::AdSpaceV1;
        ad_space.creator = creator;
        ad_space.store_hash = store_hash;
        ad_space.slot = slot;
        
        Ok(())
      }


      pub fn create_ad(ctx: Context<CreateAd>, slot:u64, ad_config:AdConfig) -> Result<()> {
        
        let ad_creator = &mut ctx.accounts.ad_creator;
        let ad_creator_key = ad_creator.key();
        let ad = &mut ctx.accounts.ad;

        

        let creator = &ctx.accounts.creator;
        let creator_key = creator.key();
        let store = &ctx.accounts.store;

        let ad_info = ad.to_account_info();

        let mut creators = vec![];

        creators.push(Creator {
            address:creator_key.clone(),
            share:100,
            verified:false
        });
        
        creators.push(Creator {
            address:ad_creator_key.clone(),
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:ad_info.key(),
            share:0,
            verified:false
        });

        let clock = Clock::get().unwrap();
        let clock32 = clock.unix_timestamp.clamp(0, u32::MAX as i64) as u32;

        let mut payload:Vec<u8> = vec![];
        payload.extend(clock32.to_le_bytes());

        let mut uri = match ad_config.asset_bundler {
            AssetBundler::Arweave => {
                "https://arweave.net"
            }
            AssetBundler::Raw => {
                "https://"
            }
            _ => {
                "https://arweave.net"
            }
        }.to_string() + bytes_to_string(&ad_config.arweave).as_str();
        
        if payload.len() > 0 {
            uri += "?p=";
            uri += encode_bytes_to_ascii_string(&payload, true).as_str();
        }

        let metadata = MetadataArgs {
            name:"Ad V0".to_string(),
            symbol:encode_u64_to_ascii_string(ad_creator.created_spots as u64),
            uri,
            seller_fee_basis_points: 0,
            creators,
            primary_sale_happened: false,
            is_mutable: true,
            edition_nonce: None,
            collection: Some(Collection { verified:true, key:ctx.accounts.collection_mint.key() }),
            uses: None,
            token_standard: Some(TokenStandard::NonFungible),
            token_program_version: TokenProgramVersion::Original,
        };

        let merkle_manager_bump = &ctx.bumps.merkle_manager.to_le_bytes();
        let merkle_manager = &[
            b"tree".as_ref(),
            merkle_manager_bump
        ];

        let signature:Vec<&[&[u8]]> = vec![
            merkle_manager
        ];
        
        let bubblegum_program_info = ctx.accounts.bubblegum_program.to_account_info();
        let tree_authority_info = ctx.accounts.tree_authority.to_account_info();
        let merkle_tree_info = ctx.accounts.merkle_tree.to_account_info();
        let compression_program_info = ctx.accounts.compression_program.to_account_info();
        let log_wrapper_info = ctx.accounts.log_wrapper.to_account_info();
        let merkle_manager_info = ctx.accounts.merkle_manager.to_account_info();
        
        
        let edition_account_info = ctx.accounts.edition_account.to_account_info();
        let collection_mint_info = ctx.accounts.collection_mint.to_account_info();
        let bubblegum_signer_info = ctx.accounts.bubblegum_signer.to_account_info();
        let collection_metadata_info = ctx.accounts.collection_metadata.to_account_info();
        let token_metadata_program_info = ctx.accounts.token_metadata_program.to_account_info();
        let system_program_info = ctx.accounts.system_program.to_account_info();


        let ad_creator_info = ad_creator.to_account_info();
        let creator_info = creator.to_account_info();

        let result = mint_to_collection_cnft(
        &bubblegum_program_info,
        &tree_authority_info,
        &ad_creator_info,
        &ad_creator_info,
        &merkle_tree_info,
        &creator_info,
        &merkle_manager_info,
        &merkle_manager_info,
        &bubblegum_program_info,
        &collection_mint_info,
        &collection_metadata_info,
        &edition_account_info,
        &bubblegum_signer_info,
        &log_wrapper_info,
        &compression_program_info,
        &token_metadata_program_info,
        &system_program_info,
        metadata,
        &signature,
        &[]);
        
        match result {
            Ok(()) => {},
            Err(err) => return Err(err),
        }
        
        let tree_data = tree_authority_info.try_borrow_data()?;
        let num_minted = u64::from_le_bytes(tree_data[80..88].try_into().unwrap());
        let asset_id = get_asset_id(&ctx.accounts.merkle_tree.key(), num_minted - 1);
        
        ad_creator.created_spots += 1;
        ad_creator.living_spots += 1;

        {

            let mut ref_data = ad_info.try_borrow_mut_data()?;
            ref_data[8] = 4;
            ref_data[9] = 1;
            ref_data[10..42].copy_from_slice(&creator_key.to_bytes());
            ref_data[42..50].copy_from_slice(&store.store_hash.to_le_bytes());
            ref_data[50..58].copy_from_slice(&slot.to_le_bytes());
            ref_data[58..90].copy_from_slice(&asset_id.to_bytes());
            ref_data[90..90+AD_CONFIG_SIZE].copy_from_slice(&ad_config.try_to_vec()?);
            //ref_data[90+AD_CONFIG_SIZE..90+AD_CONFIG_SIZE+AD_MAIN_STATS_SIZE].copy_from_slice(&ad_config.try_to_vec()?);
            
        }
        let ad_stats_info = &mut ctx.accounts.ad_stats.to_account_info();
        {
            let mut ref_data = ad_stats_info.try_borrow_mut_data()?;
            ref_data[8] = 6;
            ref_data[9] = 1;
            ref_data[10..42].copy_from_slice(&ad_info.key().to_bytes());
            ref_data[42..42+AD_STATS_SIZE].copy_from_slice(&[0;AD_STATS_SIZE]);
        }

        Ok(())

      }
}

const MIN_SPACE_CREATOR:usize = 233;


#[derive(Accounts)]
pub struct RegisterSpaceCreator<'info> {
    #[account(
        init_if_needed,
        seeds = [b"space_creator".as_ref(), creator.key().as_ref(), &store.store_hash.to_le_bytes().as_ref()],
        bump,
        payer = creator,
        space = 8+MIN_SPACE_CREATOR
    )]
    pub space_creator: Box<Account<'info, SpaceCreator>>,
    /// CHECK: unsafe
    pub ad_creator_manager: UncheckedAccount<'info>,
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

const MIN_AD_CREATOR:usize = 233;
#[derive(Accounts)]
pub struct RegisterAdCreator<'info> {
    #[account(
        init_if_needed,
        seeds = [b"ad_creator".as_ref(), creator.key().as_ref(), &store.store_hash.to_le_bytes().as_ref()],
        bump,
        payer = creator,
        space = 8+MIN_AD_CREATOR
    )]
    pub ad_creator: Box<Account<'info, AdCreator>>,
    /// CHECK: unsafe
    pub ad_creator_manager: Option<UncheckedAccount<'info>>,
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}


#[commit]
#[derive(Accounts)]
pub struct PrepareDeleteAdCampaign<'info> {
    #[account(
        seeds = [b"ad_campaign".as_ref(), space_creator.creator.as_ref(), &store.store_hash.to_le_bytes().as_ref(),
        {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            let d:[u8;32] = ref_data[TO_AD_CAMPAIGN_TWITTER..TO_AD_CAMPAIGN_TWITTER+32].try_into().unwrap();
            d
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_AD_CAMPAIGN_SLOT..TO_AD_CAMPAIGN_SLOT+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub ad_campaign: AccountLoader<'info, AdCampaign>,
    #[account(
        seeds = [b"space_creator".as_ref(), space_creator.creator.key().as_ref(), &space_creator.store_hash.to_le_bytes().as_ref()],
        bump
    )]
    pub space_creator: Box<Account<'info, SpaceCreator>>,
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: AccountLoader<'info, AdCampaignBidTracks>,

    pub store: Box<Account<'info, Store>>,

    /*/// CHECK: unsafe
    pub delegation_metadata: UncheckedAccount<'info>,
    /// CHECK: unsafe
    pub delegation_record: UncheckedAccount<'info>,
    /// CHECK: unsafe
    pub delegation_buffer: UncheckedAccount<'info>,
    /// CHECK: unsafe
    pub delegation_program: UncheckedAccount<'info>,*/

    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(cnft:CampaignCnft)]
pub struct DeleteAdCampaign<'info> {
    #[account(
        mut,
        seeds = [b"ad_campaign".as_ref(), creator.key().as_ref(), &store.store_hash.to_le_bytes().as_ref(),{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            let d:[u8;32] = ref_data[TO_AD_CAMPAIGN_TWITTER..TO_AD_CAMPAIGN_TWITTER+32].try_into().unwrap();
            d
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_AD_CAMPAIGN_SLOT..TO_AD_CAMPAIGN_SLOT+4].try_into().unwrap())
        }.to_le_bytes()],
        bump,
        close = creator
    )]
    pub ad_campaign: AccountLoader<'info, AdCampaign>,

    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump,
        close = creator
    )]
    pub ad_campaign_bid_tracks: AccountLoader<'info, AdCampaignBidTracks>,

    /*#[account(
        mut,
        seeds = [b"ad_bids".as_ref(), ad_campaign.key().as_ref()],
        bump,
        close = creator
    )]
    pub ad_bids: AccountLoader<'info, AdBids>,*/


    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(
        mut,
        seeds = [b"space_creator".as_ref(), creator.key().as_ref(), &cyrb53_bytes(&store.key().to_bytes(),0).to_le_bytes().as_ref()],
        bump
    )]
    pub space_creator: Box<Account<'info, SpaceCreator>>,
    #[account(mut)]
    pub creator: Signer<'info>,

    /// CHECK: unsafe
    #[account(mut)]
    pub merkle_tree: UncheckedAccount<'info>, 
    #[account(mut)]
    /// CHECK: unsafe
    pub tree_authority: UncheckedAccount<'info>,
    #[account( mut, seeds = [b"tree".as_ref()], bump )]
    /// CHECK: unsafe
    pub merkle_manager: UncheckedAccount<'info>, 
    /// CHECK: Optional collection authority record PDA.
    pub collection_authority_record_pda:UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub edition_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: This account is checked in the instruction
    pub collection_metadata: UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub collection_mint: UncheckedAccount<'info>,
    /// CHECK: This is just used as a signing PDA.
    #[account()]
    pub bubblegum_signer: UncheckedAccount<'info>,
    pub log_wrapper: Program<'info, Noop>,
    pub token_metadata_program: Program<'info, MplTokenMetadata>,
    pub bubblegum_program: Program<'info, Bubblegum>,
    pub compression_program: Program<'info, SplAccountCompression>,
    
    pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
#[instruction(cnft:CampaignCnft)]
pub struct DeleteAdSpot<'info> {
    #[account(
        mut,
        seeds = [b"ad_spot".as_ref(), creator.key().as_ref(), &store.store_hash.to_le_bytes().as_ref(), &ad_spot.slot.to_le_bytes()],
        bump,
        close = creator
    )]
    pub ad_spot: Box<Account<'info, AdSpot>>,

    
    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(
        mut,
        seeds = [b"ad_creator".as_ref(), creator.key().as_ref(), &cyrb53_bytes(&store.key().to_bytes(),0).to_le_bytes().as_ref()],
        bump
    )]
    pub ad_creator: Box<Account<'info, AdCreator>>,
    #[account(mut)]
    pub creator: Signer<'info>,

    /// CHECK: unsafe
    #[account(mut)]
    pub merkle_tree: UncheckedAccount<'info>, 
    #[account(mut)]
    /// CHECK: unsafe
    pub tree_authority: UncheckedAccount<'info>,
    #[account( mut, seeds = [b"tree".as_ref()], bump )]
    /// CHECK: unsafe
    pub merkle_manager: UncheckedAccount<'info>, 
    /// CHECK: Optional collection authority record PDA.
    pub collection_authority_record_pda:UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub edition_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: This account is checked in the instruction
    pub collection_metadata: UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub collection_mint: UncheckedAccount<'info>,
    /// CHECK: This is just used as a signing PDA.
    #[account()]
    pub bubblegum_signer: UncheckedAccount<'info>,
    pub log_wrapper: Program<'info, Noop>,
    pub token_metadata_program: Program<'info, MplTokenMetadata>,
    pub bubblegum_program: Program<'info, Bubblegum>,
    pub compression_program: Program<'info, SplAccountCompression>,
    
    pub system_program: Program<'info, System>,
}


const MIN_AD_SPOT:usize = 241;


#[derive(Accounts)]
#[instruction(slot:u32, asset_source:AssetSource)]
pub struct RegisterAdSpot<'info> {
    #[account(
        init,
        seeds = [b"ad_spot".as_ref(), creator.key().as_ref(), &store.store_hash.to_le_bytes().as_ref(), &slot.to_le_bytes()],
        bump,
        payer = creator,
        space = 8+MIN_AD_SPOT
    )]
    pub ad_spot: Box<Account<'info, AdSpot>>,
    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(
        mut,
        seeds = [b"ad_creator".as_ref(), creator.key().as_ref(), &cyrb53_bytes(&store.key().to_bytes(),0).to_le_bytes().as_ref()],
        bump
    )]
    pub ad_creator: Box<Account<'info, AdCreator>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    
    /// CHECK: unsafe
    #[account(mut)]
    pub merkle_tree: UncheckedAccount<'info>, 
    #[account(mut)]
    /// CHECK: unsafe
    pub tree_authority: UncheckedAccount<'info>,
    #[account( mut, seeds = [b"tree".as_ref()], bump )]
    /// CHECK: unsafe
    pub merkle_manager: UncheckedAccount<'info>, 
    /// CHECK: Optional collection authority record PDA.
    pub collection_authority_record_pda:UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub edition_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: This account is checked in the instruction
    pub collection_metadata: UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub collection_mint: UncheckedAccount<'info>,
    /// CHECK: This is just used as a signing PDA.
    #[account()]
    pub bubblegum_signer: UncheckedAccount<'info>,
    pub log_wrapper: Program<'info, Noop>,
    pub token_metadata_program: Program<'info, MplTokenMetadata>,
    pub bubblegum_program: Program<'info, Bubblegum>,
    pub compression_program: Program<'info, SplAccountCompression>,

    pub system_program: Program<'info, System>,
}

const MIN_AD_SPACE:usize = 233;


pub const START_FROM_AD_CAMPAIGN_BID_TRACKS:usize = 8 + 110;
pub const SPACE_IN_CAMPAIGN_BID_TRACK:usize = 22;

pub const AD_CAMPAIGN_BID_TRACK_DELETING_STATE:u8 = 10;
pub const AD_CAMPAIGN_BID_TRACK_DELEGATING_STATE:u8 = 9;

pub const TO_AD_CAMPAIGN_BID_TRACK_CREATOR:usize = 17;
pub const TO_AD_CAMPAIGN_BID_TRACK_CAMPAIGN:usize = 49;
pub const TO_AD_CAMPAIGN_BID_EARNED:usize = 90;
pub const TO_AD_CAMPAIGN_BID_TRACK_DELEGATION_STATE:usize = 89;
pub const TO_AD_CAMPAIGN_BID_TRACK_STORE_HASH:usize = 9;
pub const TO_AD_CAMPAIGN_BID_TRACK_SPACES:usize = 114;
pub const TO_AD_CAMPAIGN_BID_TRACK_LIVING:usize = 110;
pub const TO_AD_CAMPAIGN_BID_TRACK_CHANGE_COUNT:usize = 81;

#[account]
//110
pub struct AdCampaignBidTrackStruct {
  pub class:AccountClass, //1
  pub store_hash:u64, //8
  pub creator: Pubkey, //32
  pub campaign: Pubkey, //32
  pub changes_count:u64, //8
  pub state:u8, //1
  pub earned:u64, //8
  pub extra:[u8;12], //12
  pub living_bids:u32, //4
  pub spaces_bids:u32 //4
}


#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
//22
pub struct AdCampaignBidTrackPieceStruct {
  pub state:u8, //1
  pub has_budget:u8, //1
  pub budget_per_view: u64, //8
  pub budget_per_click: u64, //8
  pub last_view:u32, //4
}

pub const AD_CAMPAIGN_BID_STATE_CASHINGOUT:u8 = 20;
pub const AD_CAMPAIGN_BID_STATE_CASHINGOUT_PAUSED:u8 = 20;

pub const AD_CAMPAIGN_BID_STATE_CREATING:u8 = 3;
pub const AD_CAMPAIGN_BID_STATE_READY:u8 = 1;
pub const AD_CAMPAIGN_BID_STATE_PAUSED:u8 = 2;

pub const AD_CAMPAIGN_BID_STATE_DELETING:u8 = 22;
pub const AD_CAMPAIGN_BID_STATE_DELETING_PAUSED:u8 = 23;


pub const START_FROM_AD_CAMPAIGN:usize = 8 + 213;
pub const SPACE_IN_CAMPAIGN_BID:usize = 1;

pub const TO_AD_CAMPAIGN_CREATOR:usize = 17;
pub const TO_AD_CAMPAIGN_STORE_HASH:usize = 9;
pub const TO_AD_CAMPAIGN_TWITTER:usize = 49;
pub const TO_AD_CAMPAIGN_SLOT:usize = 81;
pub const TO_AD_CAMPAIGN_CONFIG:usize = 109;
pub const TO_AD_CAMPAIGN_CONFIG_CURRENCY:usize = 109+18;
pub const AD_CAMPAIGN_CONFIG_SIZE:usize = 52;
pub const TO_AD_CAMPAIGN_SPACES_BID:usize = 217;
pub const TO_AD_CAMPAIGN_LIVING_BID:usize = 213;
pub const TO_AD_CAMPAIGN_CHANGE_COUNT:usize = TO_AD_CAMPAIGN_CONFIG+AD_CAMPAIGN_CONFIG_SIZE;


#[account]
//213
pub struct AdCampaignStruct {
  pub class:AccountClass, //1
  pub store_hash:u64, //8
  pub creator: Pubkey, //32
  pub twitter:[u8;32], //32
  pub slot:u32, //4
  pub extra_pre:[u8;24], //24
  pub config:CampaignConfig, //52
  pub change_counts:u64,
  pub extra_post:[u8;40], //40
  pub created_bids:u32, //4
  pub living_bids:u32, //4
  pub spaces_bids:u32, //4
}


#[account(zero_copy)]
#[repr(C)]
pub struct AdCampaign {
  /*
  */
  pub data:[u8;10_000_000]
}

#[account(zero_copy)]
#[repr(C)]
pub struct AdCampaignBidTracks {
  /*
  */
  pub data:[u8;10_000_000]
}

#[delegate]
#[derive(Accounts)]
#[instruction(twitter_proof:[u8;64], slot:u32, campaign_config:CampaignConfig, twitter:[u8;32], asset_source:AssetSource)]
pub struct RegisterAdCampaign<'info> {
    #[account(
        init,
        seeds = [b"ad_campaign".as_ref(), creator.key().as_ref(), &store.store_hash.to_le_bytes().as_ref(), twitter.as_ref(), &slot.to_le_bytes()],
        bump,
        payer = creator,
        space = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID
    )]
    pub ad_campaign: AccountLoader<'info, AdCampaign>,

    /// CHECK: unsafe
    #[account(
        mut,
        del,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,

    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(
        mut,
        seeds = [b"space_creator".as_ref(), creator.key().as_ref(), &cyrb53_bytes(&store.key().to_bytes(),0).to_le_bytes().as_ref()],
        bump
    )]
    pub space_creator: Box<Account<'info, SpaceCreator>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    
    /// CHECK: unsafe
    #[account(mut)]
    pub merkle_tree: UncheckedAccount<'info>, 
    #[account(mut)]
    /// CHECK: unsafe
    pub tree_authority: UncheckedAccount<'info>,
    #[account( mut, seeds = [b"tree".as_ref()], bump )]
    /// CHECK: unsafe
    pub merkle_manager: UncheckedAccount<'info>, 
    /// CHECK: Optional collection authority record PDA.
    pub collection_authority_record_pda:UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub edition_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: This account is checked in the instruction
    pub collection_metadata: UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub collection_mint: UncheckedAccount<'info>,
    /// CHECK: This is just used as a signing PDA.
    #[account()]
    pub bubblegum_signer: UncheckedAccount<'info>,
    pub log_wrapper: Program<'info, Noop>,
    pub token_metadata_program: Program<'info, MplTokenMetadata>,
    pub bubblegum_program: Program<'info, Bubblegum>,
    pub compression_program: Program<'info, SplAccountCompression>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(slot:u32)]
pub struct RegisterAdSpace<'info> {
    #[account(
        init,
        seeds = [b"ad_space".as_ref(), creator.key().as_ref(), &store.store_hash.to_le_bytes().as_ref(), &slot.to_le_bytes()],
        bump,
        payer = creator,
        space = 8+MIN_AD_SPACE
    )]
    pub ad_space: Box<Account<'info, AdSpace>>,
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

const START_FROM_AD:usize = 90+AD_CONFIG_SIZE+AD_MAIN_STATS_SIZE;
const SPACE_IN_AD:usize = 233;


const START_FROM_AD_STATS:usize = 42+AD_STATS_SIZE;
const SPACE_IN_AD_STAS:usize = 233;

#[derive(Accounts)]
#[instruction(slot:u64, ad_config:AdConfig)]
pub struct CreateAd<'info> {
    #[account(
        init,
        seeds = [b"ad".as_ref(), ad_creator.key().as_ref(), &slot.to_le_bytes()],
        bump,
        payer = creator,
        space = 8 + START_FROM_AD + SPACE_IN_AD
    )]
    pub ad: AccountLoader<'info, Ad>,
    /// CHECK
    #[account(
        init,
        seeds = [b"ad_stats".as_ref(), ad.key().as_ref()],
        bump,
        payer = creator,
        space = 8 + START_FROM_AD_STATS
    )]
    pub ad_stats: AccountLoader<'info, AdStat>,
    #[account(
        mut,
        seeds = [b"ad_creator".as_ref(), creator.key().as_ref(), &cyrb53_bytes(&store.key().to_bytes(),0).to_le_bytes().as_ref()],
        bump
    )]
    pub ad_creator: Box<Account<'info, AdCreator>>,
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,

    
    /// CHECK: unsafe
    #[account(mut)]
    pub merkle_tree: UncheckedAccount<'info>, 
    #[account(mut)]
    /// CHECK: unsafe
    pub tree_authority: UncheckedAccount<'info>,
    #[account( mut, seeds = [b"tree".as_ref()], bump )]
    /// CHECK: unsafe
    pub merkle_manager: UncheckedAccount<'info>, 
    /// CHECK: Optional collection authority record PDA.
    pub collection_authority_record_pda:UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub edition_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: This account is checked in the instruction
    pub collection_metadata: UncheckedAccount<'info>,
    /// CHECK: This account is checked in the instruction
    pub collection_mint: UncheckedAccount<'info>,
    /// CHECK: This is just used as a signing PDA.
    #[account()]
    pub bubblegum_signer: UncheckedAccount<'info>,
    

    pub log_wrapper: Program<'info, Noop>,
    pub token_metadata_program: Program<'info, MplTokenMetadata>,
    pub bubblegum_program: Program<'info, Bubblegum>,
    pub compression_program: Program<'info, SplAccountCompression>,

    pub system_program: Program<'info, System>,
}

#[account(zero_copy)]
#[repr(C)]
pub struct AdStat {
  /*
  */
  pub data:[u8;10_000_000]
}

/*
#[account]
//233
pub struct AdStats {
  pub class:AccountClass, //1
  pub ad: Pubkey, //32
  pub total_visits:u64, //8
  pub unique_visits:u64, //8
}*/

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
#[repr(u8)]
pub enum PermissionType {
  Open = 0,
  Request = 1,
  VerifiedPass = 2,
  VerifiedOnly = 3
}

const AD_STATS_SIZE:usize = 40;


const AD_CONFIG_SIZE:usize = 68;
const AD_MAIN_STATS_SIZE:usize = 4;

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct AdConfig
{ //68
    pub category: AdCategory, //1
    pub budget: AdBudget, //16
    pub asset_bundler:AssetBundler, //1
    pub arweave:[u8;50], //50
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct AdMainStats
{ //4
    pub in_spaces: u32, //4
}


#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct AdBudget
{ //16
    pub total: u64, //8
    pub daily:u64, //8
}

#[account(zero_copy)]
#[repr(C)]
pub struct Ad {
  /*
  */
  pub data:[u8;10_000_000]
}

#[account]
//233
pub struct AdCreator {
  pub class:AccountClass, //1
  pub creator: Pubkey, //32
  pub store_hash:u64, //8
  pub created_spots:u32, //4
  pub living_spots:u32, //4
  pub created_bids:u32, //4
  pub living_bids:u32, //4
  pub spent:u64, //8
  pub creator_manager:Pubkey,
  pub extra:[u8;136] //136
}

#[account]
//233
pub struct SpaceCreator {
  pub class:AccountClass, //1
  pub creator: Pubkey, //32
  pub store_hash:u64, //8
  pub created:u32, //4
  pub living:u32, //4
  pub earning:u64, //8
  pub creator_manager:Pubkey,
  pub extra:[u8;144] //144
}


#[account]
//233
pub struct AdSpace {
  pub class:AccountClass, //1
  pub creator: Pubkey, //32
  pub store_hash:u64, //8
  pub slot:u32, //4
  pub extra:[u8;188] //188
}


#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct CampaignConfig
{ //52
    pub max_seconds: u8, //1
    pub permission: PermissionType, //1
    pub categories: [AdCategory;16], //16
    pub currency: Pubkey, //32
    pub max_bidders:u16,
}




#[account]
//241
pub struct AdSpot {
  pub class:AccountClass, //1
  pub store_hash:u64, //8
  pub creator: Pubkey, //32
  pub slot:u32, //4
  pub live_bids:u32, //4
  pub extra:[u8;192] //192
}



#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct AssetSource
{
    pub arweave:String, //50
    pub asset_bundler:AssetBundler, //1
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct CampaignCnft
{
    pub root: [u8;32],
    pub data_hash:[u8;32],
    pub index:u32
}

