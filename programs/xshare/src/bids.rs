use anchor_lang::prelude::*;
use crate::AccountClass;
use crate::utils::{close_account,pay_to_user,encode_bytes_to_ascii_string, encode_u64_to_ascii_string};
use crate::base::{MplTokenMetadata,Store};
use anchor_lang::system_program::{create_account, CreateAccount};
use crate::ads::{AdCampaign, AdSpace, AdCreator,SpaceCreator, AdSpot,AD_CAMPAIGN_BID_STATE_PAUSED,AD_CAMPAIGN_BID_STATE_CASHINGOUT_PAUSED,AD_CAMPAIGN_BID_STATE_DELETING_PAUSED,AD_CAMPAIGN_BID_STATE_DELETING,AD_CAMPAIGN_BID_STATE_CASHINGOUT,TO_AD_CAMPAIGN_BID_EARNED,TO_AD_CAMPAIGN_LIVING_BID,DEVNET_VALIDATOR,TO_AD_CAMPAIGN_CONFIG_CURRENCY,TO_AD_CAMPAIGN_TWITTER,TO_AD_CAMPAIGN_BID_TRACK_CHANGE_COUNT,TO_AD_CAMPAIGN_CHANGE_COUNT,AD_CAMPAIGN_BID_STATE_READY,AD_CAMPAIGN_BID_STATE_CREATING,START_FROM_AD_CAMPAIGN_BID_TRACKS,SPACE_IN_CAMPAIGN_BID_TRACK,START_FROM_AD_CAMPAIGN,TO_AD_CAMPAIGN_SPACES_BID,SPACE_IN_CAMPAIGN_BID,TO_AD_CAMPAIGN_BID_TRACK_SPACES,TO_AD_CAMPAIGN_BID_TRACK_LIVING,TO_AD_CAMPAIGN_CREATOR,TO_AD_CAMPAIGN_STORE_HASH,TO_AD_CAMPAIGN_SLOT,AdCampaignBidTrackPieceStruct};

use crate::cnft::{mint_to_collection_cnft,burn_cnft,MetadataArgs,AssetBundler,Creator,SplAccountCompression, Bubblegum, Collection, TokenStandard, TokenProgramVersion,Noop};

const BID_TRACK_DISCRIMINATOR:[u8;8] = [120,  37, 248, 129,   6, 97,  52,  91];
const ROLLUP_MESSENGER_DISCRIMINATOR:[u8;8] = [115,  14,  78, 217,  78, 139, 183, 233];

use ephemeral_rollups_sdk::anchor::{commit, delegate, ephemeral};

use crate::bloom::{BLOOM_HASH_COUNT,hash,optimal_bloom_parameters};

use ephemeral_rollups_sdk::cpi::{DelegateConfig,DelegateAccounts, delegate_account};
use ephemeral_rollups_sdk::ephem::{commit_accounts, commit_and_undelegate_accounts};

pub mod bids_ix {
    use std::f32::consts::E;

    use super::*;

    pub fn commit_cashout(ctx: Context<CommitCashout>) -> Result<()> {

        let bid_index = {
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ref_data = bid_track_info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        };

        let needs_to_update = {
            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;
            if ref_data[MESSENGER_TO_STATE] == MESSENGER_DELEGATED_CASHOUT {
                ref_data[MESSENGER_TO_STATE] = MESSENGER_PROCESSED_CASHOUT;

                RollupUpdates::try_from_slice(&ref_data[MESSENGER_TO_PAYLOAD..MESSENGER_TO_PAYLOAD+MESSAGE_PAYLOAD_SIZE])?

            } else {
                return Err(ProgramError::InvalidAccountData.into());
            }
        };

        let ad_campaign_bid_tracks_key = ctx.accounts.ad_campaign_bid_tracks.key();

        let changes_count = match needs_to_update {
            RollupUpdates::CashoutBid { account, changes_count, index, earned } => {
                if bid_index != index || account != ad_campaign_bid_tracks_key {
                    return Err(ProgramError::InvalidAccountData.into());    
                }
                changes_count
            }
            _ => {
                return Err(ProgramError::InvalidAccountData.into());
            }
        };

        let ad_campaign = &ctx.accounts.ad_campaign;

        {
            let ad_campaign_info = ad_campaign.to_account_info();
            let ref_data_ad_campaign = ad_campaign_info.try_borrow_data()?;

            let pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * bid_index as usize;
            if ref_data_ad_campaign[pos] != AD_CAMPAIGN_BID_STATE_CASHINGOUT && ref_data_ad_campaign[pos] != AD_CAMPAIGN_BID_STATE_CASHINGOUT_PAUSED {
                return Err(ProgramError::InvalidAccountData.into());
            }
        }

        let (should_have_earned, lost_all_budget) = {

            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let mut ref_data = bid_track_info.try_borrow_mut_data()?;

            let cleaning_views = u64::from_le_bytes(ref_data[TO_BID_TRACK_TOTAL_VIEWS..TO_BID_TRACK_TOTAL_VIEWS+8].try_into().unwrap());
            let cleaning_click = u64::from_le_bytes(ref_data[TO_BID_TRACK_TOTAL_CLICKS..TO_BID_TRACK_TOTAL_CLICKS+8].try_into().unwrap());

            let mut bid_config = BidConfig::try_from_slice(&ref_data[TO_BID_TRACK_AD_CONFIG..TO_BID_TRACK_AD_CONFIG+BID_TRACK_CONFIG_SIZE])?;

            let spent_per_view = cleaning_views * bid_config.budget_per_view;
            let spent_per_click = cleaning_click * bid_config.budget_per_click;

            let should_have_earned = spent_per_view+spent_per_click;

            //remove money from budget
            if should_have_earned > bid_config.budget {
                bid_config.budget = 0;
            } else {
                bid_config.budget = bid_config.budget - should_have_earned;
            }

            //clean clicks and views
            ref_data[TO_BID_TRACK_TOTAL_VIEWS..TO_BID_TRACK_TOTAL_VIEWS+8].copy_from_slice(&(0 as u64).to_be_bytes());
            ref_data[TO_BID_TRACK_TOTAL_CLICKS..TO_BID_TRACK_TOTAL_CLICKS+8].copy_from_slice(&(0 as u64).to_be_bytes());

            ref_data[TO_BID_TRACK_AD_CONFIG..TO_BID_TRACK_AD_CONFIG+BID_TRACK_CONFIG_SIZE].copy_from_slice(&bid_config.try_to_vec()?);

            (should_have_earned, bid_config.budget==0)

        };

        {
            let info_campaign_bid_tracks = ctx.accounts.ad_campaign_bid_tracks.to_account_info();
            let mut ref_data = info_campaign_bid_tracks.try_borrow_mut_data()?;

            ref_data[TO_AD_CAMPAIGN_BID_TRACK_CHANGE_COUNT..TO_AD_CAMPAIGN_BID_TRACK_CHANGE_COUNT+8].copy_from_slice(&changes_count.to_le_bytes());


            let bid_pos = START_FROM_AD_CAMPAIGN_BID_TRACKS + SPACE_IN_CAMPAIGN_BID_TRACK * bid_index as usize;

            if ref_data[bid_pos] != 1 {
                msg!("unsynced");
                return Err(ProgramError::InvalidAccountData.into());
            }

            if lost_all_budget {
                ref_data[bid_pos+1] = 0;
            }

            let total_earned = u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_BID_EARNED..TO_AD_CAMPAIGN_BID_EARNED+8].try_into().unwrap());

            let remaning_earned_cash = total_earned - should_have_earned;

            //subtract the amount earned from this bid
            ref_data[TO_AD_CAMPAIGN_BID_EARNED..TO_AD_CAMPAIGN_BID_EARNED+8].copy_from_slice(&(remaning_earned_cash as u64).to_be_bytes());


        }

        {

            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            pay_to_user(&bid_track_info, &bid_messenger_info, should_have_earned)?;

        }

        {
            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();

            let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;

            ref_data[MESSENGER_TO_CASHOUT_EARNED..MESSENGER_TO_CASHOUT_EARNED+8].copy_from_slice(&should_have_earned.to_le_bytes());

        }

        {


            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();

            let good = commit_and_undelegate_accounts(
                &ctx.accounts.creator.to_account_info(),
                vec![&bid_messenger_info],
                &ctx.accounts.magic_context,
                &ctx.accounts.magic_program,
            );

            match good {
                Ok(()) => {
                }
                Err(err) => {
                    // Handle error case  
                    msg!("err {:?}",err);
                    return Err(ProgramError::InvalidArgument.into())
                }
            } 

        }

        Ok(())
    }

    

    pub fn commit_delete_bid(ctx: Context<CommitDeleteBid>) -> Result<()> {

        let bid_index = {
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ref_data = bid_track_info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        };

        let needs_to_update = {
            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;
            if ref_data[MESSENGER_TO_STATE] == MESSENGER_DELEGATED_CASHOUT {
                ref_data[MESSENGER_TO_STATE] = MESSENGER_PROCESSED_CASHOUT;

                RollupUpdates::try_from_slice(&ref_data[MESSENGER_TO_PAYLOAD..MESSENGER_TO_PAYLOAD+MESSAGE_PAYLOAD_SIZE])?

            } else {
                return Err(ProgramError::InvalidAccountData.into());
            }
        };

        let ad_campaign_bid_tracks_key = ctx.accounts.ad_campaign_bid_tracks.key();

        let changes_count = match needs_to_update {
            RollupUpdates::DeletingBid { account, changes_count, index, must_pay_user } => {
                if bid_index != index || account != ad_campaign_bid_tracks_key {
                    return Err(ProgramError::InvalidAccountData.into());    
                }
                changes_count
            }
            _ => {
                return Err(ProgramError::InvalidAccountData.into());
            }
        };

        let ad_campaign = &ctx.accounts.ad_campaign;

        {
            let ad_campaign_info = ad_campaign.to_account_info();
            let ref_data_ad_campaign = ad_campaign_info.try_borrow_data()?;

            let pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * bid_index as usize;
            if ref_data_ad_campaign[pos] != AD_CAMPAIGN_BID_STATE_DELETING && ref_data_ad_campaign[pos] != AD_CAMPAIGN_BID_STATE_DELETING_PAUSED {
                return Err(ProgramError::InvalidAccountData.into());
            }
        }

        let (should_have_earned, lost_all_budget) = {

            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let mut ref_data = bid_track_info.try_borrow_mut_data()?;

            let cleaning_views = u64::from_le_bytes(ref_data[TO_BID_TRACK_TOTAL_VIEWS..TO_BID_TRACK_TOTAL_VIEWS+8].try_into().unwrap());
            let cleaning_click = u64::from_le_bytes(ref_data[TO_BID_TRACK_TOTAL_CLICKS..TO_BID_TRACK_TOTAL_CLICKS+8].try_into().unwrap());

            let mut bid_config = BidConfig::try_from_slice(&ref_data[TO_BID_TRACK_AD_CONFIG..TO_BID_TRACK_AD_CONFIG+BID_TRACK_CONFIG_SIZE])?;

            let spent_per_view = cleaning_views * bid_config.budget_per_view;
            let spent_per_click = cleaning_click * bid_config.budget_per_click;

            let should_have_earned = spent_per_view+spent_per_click;

            //remove money from budget
            if should_have_earned > bid_config.budget {
                bid_config.budget = 0;
            } else {
                bid_config.budget = bid_config.budget - should_have_earned;
            }
            

            //clean clicks and views
            ref_data[TO_BID_TRACK_TOTAL_VIEWS..TO_BID_TRACK_TOTAL_VIEWS+8].copy_from_slice(&(0 as u64).to_be_bytes());
            ref_data[TO_BID_TRACK_TOTAL_CLICKS..TO_BID_TRACK_TOTAL_CLICKS+8].copy_from_slice(&(0 as u64).to_be_bytes());

            ref_data[TO_BID_TRACK_AD_CONFIG..TO_BID_TRACK_AD_CONFIG+BID_TRACK_CONFIG_SIZE].copy_from_slice(&bid_config.try_to_vec()?);

            (should_have_earned, bid_config.budget==0)

        };

        {
            let info_campaign_bid_tracks = ctx.accounts.ad_campaign_bid_tracks.to_account_info();
            let mut ref_data = info_campaign_bid_tracks.try_borrow_mut_data()?;

            ref_data[TO_AD_CAMPAIGN_BID_TRACK_CHANGE_COUNT..TO_AD_CAMPAIGN_BID_TRACK_CHANGE_COUNT+8].copy_from_slice(&changes_count.to_le_bytes());

            let bid_pos = START_FROM_AD_CAMPAIGN_BID_TRACKS + SPACE_IN_CAMPAIGN_BID_TRACK * bid_index as usize;

            if ref_data[bid_pos] != 1 {
                msg!("unsynced");
                return Err(ProgramError::InvalidAccountData.into());
            }

            if lost_all_budget {
                ref_data[bid_pos+1] = 0;
            }

            let total_earned = u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_BID_EARNED..TO_AD_CAMPAIGN_BID_EARNED+8].try_into().unwrap());

            let remaning_earned_cash = total_earned - should_have_earned;

            //subtract the amount earned from this bid
            ref_data[TO_AD_CAMPAIGN_BID_EARNED..TO_AD_CAMPAIGN_BID_EARNED+8].copy_from_slice(&(remaning_earned_cash as u64).to_be_bytes());

        }

        {

            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            pay_to_user(&bid_track_info, &bid_messenger_info, should_have_earned)?;

        }

        {
            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();

            let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;

            ref_data[MESSENGER_TO_CASHOUT_EARNED..MESSENGER_TO_CASHOUT_EARNED+8].copy_from_slice(&should_have_earned.to_le_bytes());

        }

        {


            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();

            let good = commit_and_undelegate_accounts(
                &ctx.accounts.creator.to_account_info(),
                vec![&bid_messenger_info],
                &ctx.accounts.magic_context,
                &ctx.accounts.magic_program,
            );

            match good {
                Ok(()) => {
                }
                Err(err) => {
                    // Handle error case  
                    msg!("err {:?}",err);
                    return Err(ProgramError::InvalidArgument.into())
                }
            } 

        }

        {


            let bid_track_info = ctx.accounts.bid_track.to_account_info();

            let good = commit_and_undelegate_accounts(
                &ctx.accounts.creator.to_account_info(),
                vec![&bid_track_info],
                &ctx.accounts.magic_context,
                &ctx.accounts.magic_program,
            );

            match good {
                Ok(()) => {
                }
                Err(err) => {
                    // Handle error case  
                    msg!("err {:?}",err);
                    return Err(ProgramError::InvalidArgument.into())
                }
            } 

        }

        Ok(())
    }



    pub fn close_commit_cashout(ctx: Context<CloseCommitCashout>) -> Result<()> {

        let bid_messenger = &ctx.accounts.bid_messenger;
        let creator_key = ctx.accounts.creator.key();

        let (index,earned) = match bid_messenger.needs_to_update[0] {
            RollupUpdates::CashoutBid { account, changes_count, index, earned } => {

                let (ad_campaign_bid_tracks_key,bump) = Pubkey::find_program_address(&[b"ad_campaign_bid_tracks", ctx.accounts.ad_campaign.key().as_ref()], &crate::ID);

                if ad_campaign_bid_tracks_key != account {
                    return Err(ProgramError::InvalidAccountData.into())
                }

                (index,earned)
            }
            _ => {
                 return Err(ProgramError::InvalidAccountData.into())
            }
        };

        {
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ref_data = bid_track_info.try_borrow_data()?;

            let ad_spot = Pubkey::new_from_array(ref_data[TO_BID_TRACK_AD_SPOT..TO_BID_TRACK_AD_SPOT+32].try_into().unwrap());

            let compare_key = ctx.accounts.ad_spot.key();
            if ad_spot != compare_key {
                msg!("bad data {:?} {:?}",ad_spot,compare_key);
                return Err(ProgramError::InvalidAccountData.into())
            }
        }

        //pay earnings
        {
            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let space_creator_owner_info = ctx.accounts.space_creator_owner.to_account_info();
            pay_to_user(&bid_messenger_info, &space_creator_owner_info, earned)?;
        }

        let space_creator = &mut ctx.accounts.space_creator;
        space_creator.earning += earned;

        {

            let ad_campaign_info = ctx.accounts.ad_campaign.to_account_info();
            let mut ref_data = ad_campaign_info.try_borrow_mut_data()?;
            let pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * index as usize;
            if ref_data[pos] == AD_CAMPAIGN_BID_STATE_CASHINGOUT {
                ref_data[pos] = AD_CAMPAIGN_BID_STATE_READY;
            } else if ref_data[pos] == AD_CAMPAIGN_BID_STATE_CASHINGOUT_PAUSED {
                ref_data[pos] = AD_CAMPAIGN_BID_STATE_PAUSED;
            } else {
                msg!("bid not available");
                return Err(ProgramError::InvalidAccountData.into())
            }
            

        }

        {

            let bid_messenger_info = bid_messenger.to_account_info();
            let bid_messenger_creator_info = ctx.accounts.messenger_creator.to_account_info();
            let total_lamports_in_messenger = bid_messenger_info.lamports();
            let get_paid = 5000+10000;
            let remaining_lamports = total_lamports_in_messenger - get_paid;
            if bid_messenger.creator != creator_key {
                pay_to_user(&bid_messenger_info, &bid_messenger_creator_info, remaining_lamports)?;
            }
        }


        Ok(())
    }
    
    pub fn close_commit_delete_bid(ctx: Context<CloseCommitDeleteBid>) -> Result<()> {

        let bid_messenger = &ctx.accounts.bid_messenger;
        let creator_key = ctx.accounts.creator.key();

        let (index,must_pay_user) = match bid_messenger.needs_to_update[0] {
            RollupUpdates::DeletingBid { account, changes_count, index, must_pay_user } => {

                let (ad_campaign_bid_tracks_key,bump) = Pubkey::find_program_address(&[b"ad_campaign_bid_tracks", ctx.accounts.ad_campaign.key().as_ref()], &crate::ID);

                if ad_campaign_bid_tracks_key != account {
                    return Err(ProgramError::InvalidAccountData.into())
                }

                (index,must_pay_user)
            }
            _ => {
                 return Err(ProgramError::InvalidAccountData.into())
            }
        };

        {
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ref_data = bid_track_info.try_borrow_data()?;

            let ad_spot = Pubkey::new_from_array(ref_data[TO_BID_TRACK_AD_SPOT..TO_BID_TRACK_AD_SPOT+32].try_into().unwrap());

            let compare_key = ctx.accounts.ad_spot.key();
            if ad_spot != compare_key {
                msg!("bad data {:?} {:?}",ad_spot,compare_key);
                return Err(ProgramError::InvalidAccountData.into())
            }
        }

        //pay earnings
        {
            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let space_creator_owner_info = ctx.accounts.space_creator_owner.to_account_info();
            pay_to_user(&bid_messenger_info, &space_creator_owner_info, must_pay_user)?;
        }

        //return remaining budget
        let must_return_budget = {
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            
            let ref_data = bid_track_info.try_borrow_data()?;

            let bid_config = BidConfig::try_from_slice(&ref_data[TO_BID_TRACK_AD_CONFIG..TO_BID_TRACK_AD_CONFIG+BID_TRACK_CONFIG_SIZE])?;

            let remaining_budget = bid_config.budget;

            remaining_budget
        };

        if must_return_budget > 0 {
            
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ad_creator_owner_info = ctx.accounts.ad_creator_owner.to_account_info();
            pay_to_user(&bid_track_info, &ad_creator_owner_info, must_return_budget)?;

        }

        {

            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ad_creator_owner_info = ctx.accounts.ad_creator_owner.to_account_info();

            let good = close_account(&bid_track_info, &ad_creator_owner_info);

            match good {
                Ok(()) => {
                }
                Err(err) => {
                    // Handle error case  
                    msg!("err {:?}",err);
                    return Err(ProgramError::InvalidArgument.into())
                }
            }

        }

        let space_creator = &mut ctx.accounts.space_creator;
        space_creator.earning += must_pay_user;

        {

            let ad_campaign_info = ctx.accounts.ad_campaign.to_account_info();
            let mut ref_data = ad_campaign_info.try_borrow_mut_data()?;
            let pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * index as usize;
            if ref_data[pos] == AD_CAMPAIGN_BID_STATE_CASHINGOUT {
                ref_data[pos] = AD_CAMPAIGN_BID_STATE_READY;
            } else if ref_data[pos] == AD_CAMPAIGN_BID_STATE_CASHINGOUT_PAUSED {
                ref_data[pos] = AD_CAMPAIGN_BID_STATE_PAUSED;
            } else {
                msg!("bid not available");
                return Err(ProgramError::InvalidAccountData.into())
            }
            

        }

        {

            let bid_messenger_info = bid_messenger.to_account_info();
            let bid_messenger_creator_info = ctx.accounts.messenger_creator.to_account_info();
            let total_lamports_in_messenger = bid_messenger_info.lamports();
            let get_paid = 5000+10000;
            let remaining_lamports = total_lamports_in_messenger - get_paid;
            if bid_messenger.creator != creator_key {
                pay_to_user(&bid_messenger_info, &bid_messenger_creator_info, remaining_lamports)?;
            }
        }


        Ok(())
    }
    
    pub fn delete_bid(ctx: Context<DeleteBid>) -> Result<()> {

        let creator = &mut ctx.accounts.creator;
        let creator_info = creator.to_account_info();
        let creator_key = creator.key();
        let system_program_info = ctx.accounts.system_program.to_account_info();

        let store = &ctx.accounts.store;

        let bid_track_key = ctx.accounts.bid_track.key();

        let index = {
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ref_data = bid_track_info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        };

        let campaign_changes_count = {

            let ad_campaign_info = ctx.accounts.ad_campaign.to_account_info();
            let mut ref_data = ad_campaign_info.try_borrow_mut_data()?;

            let pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * index as usize;

            if ref_data[pos] == AD_CAMPAIGN_BID_STATE_READY {
                ref_data[pos] = AD_CAMPAIGN_BID_STATE_DELETING;
            } else if ref_data[pos] != AD_CAMPAIGN_BID_STATE_PAUSED {
                ref_data[pos] = AD_CAMPAIGN_BID_STATE_DELETING_PAUSED;
            } else {
                msg!("bid not available");
                return Err(ProgramError::InvalidAccountData.into())
            }
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_CHANGE_COUNT..TO_AD_CAMPAIGN_CHANGE_COUNT+8].try_into().unwrap()) + 1
        };

        {
            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let bid_messenger_bump = ctx.bumps.bid_messenger.to_le_bytes();

            let bytes = 8 + MESSAGE_DELETING_PAYLOAD_SIZE;
            let rent = Rent::get()?;
            let min_base = rent.minimum_balance(bytes);

            
            create_account(
                CpiContext::new_with_signer(
                system_program_info.clone(),
                CreateAccount {
                    from: creator_info.clone(),
                    to: bid_messenger_info.clone()
                },
                &[&[
                    b"rollup_bid_messenger".as_ref(),
                    bid_track_key.as_ref(),
                    &bid_messenger_bump
                ]]
                ),
                min_base,
                bytes as u64,
                &crate::ID
            )?;

            let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;

            ref_data[0..8].copy_from_slice(&ROLLUP_MESSENGER_DISCRIMINATOR);
            ref_data[8] = AccountClass::RollupMessengerV1 as u8;
            ref_data[9..17].copy_from_slice(&store.store_hash.to_le_bytes());
            ref_data[MESSENGER_TO_CREATOR..MESSENGER_TO_CREATOR+32].copy_from_slice(&creator_key.as_ref());
            ref_data[MESSENGER_TO_STATE] = MESSENGER_DELEGATED_CASHOUT;
            ref_data[MESSENGER_TO_SLOT..MESSENGER_TO_SLOT+8].copy_from_slice(&(0 as u64).to_le_bytes());

            let mut updates:Vec<RollupUpdates> = vec![];

            updates.push(RollupUpdates::DeletingBid { account: ctx.accounts.ad_campaign_bid_tracks.key(), changes_count: campaign_changes_count, index, must_pay_user:0 });

            let byted: Vec<u8> = updates.try_to_vec()?;
            ref_data[26..26+byted.len()].copy_from_slice(&byted);

        }        

        Ok(())
    }

    pub fn cashout_earnings(ctx: Context<CashoutEarnings>) -> Result<()> {

        let creator = &mut ctx.accounts.creator;
        let creator_info = creator.to_account_info();
        let creator_key = creator.key();
        let system_program_info = ctx.accounts.system_program.to_account_info();

        let store = &ctx.accounts.store;

        let bid_track_key = ctx.accounts.bid_track.key();

        let index = {
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ref_data = bid_track_info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        };

        let campaign_changes_count = {

            let ad_campaign_info = ctx.accounts.ad_campaign.to_account_info();
            let mut ref_data = ad_campaign_info.try_borrow_mut_data()?;

            let pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * index as usize;

            if ref_data[pos] == AD_CAMPAIGN_BID_STATE_PAUSED {   
                ref_data[pos] = AD_CAMPAIGN_BID_STATE_CASHINGOUT_PAUSED;
            } else if ref_data[pos] == AD_CAMPAIGN_BID_STATE_READY {
                ref_data[pos] = AD_CAMPAIGN_BID_STATE_CASHINGOUT;
            } else {
                msg!("bid not available");
                return Err(ProgramError::InvalidAccountData.into())
            }


            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_CHANGE_COUNT..TO_AD_CAMPAIGN_CHANGE_COUNT+8].try_into().unwrap()) + 1
        };

        {
            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let bid_messenger_bump = ctx.bumps.bid_messenger.to_le_bytes();

            let bytes = 8 + MESSAGE_CASHOUT_PAYLOAD_SIZE;
            let rent = Rent::get()?;
            let min_base = rent.minimum_balance(bytes);

            
            
            create_account(
                CpiContext::new_with_signer(
                system_program_info.clone(),
                CreateAccount {
                    from: creator_info.clone(),
                    to: bid_messenger_info.clone()
                },
                &[&[
                    b"rollup_bid_messenger".as_ref(),
                    bid_track_key.as_ref(),
                    creator_key.as_ref(),
                    &bid_messenger_bump
                ]]
                ),
                min_base,
                bytes as u64,
                &crate::ID
            )?;

            let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;

            ref_data[0..8].copy_from_slice(&ROLLUP_MESSENGER_DISCRIMINATOR);
            ref_data[8] = AccountClass::RollupMessengerV1 as u8;
            ref_data[9..17].copy_from_slice(&store.store_hash.to_le_bytes());
            ref_data[MESSENGER_TO_CREATOR..MESSENGER_TO_CREATOR+32].copy_from_slice(&creator_key.as_ref());
            ref_data[MESSENGER_TO_STATE] = MESSENGER_DELEGATED_CASHOUT;
            ref_data[MESSENGER_TO_SLOT..MESSENGER_TO_SLOT+8].copy_from_slice(&(0 as u64).to_le_bytes());

            let mut updates:Vec<RollupUpdates> = vec![];

            updates.push(RollupUpdates::CashoutBid { account: ctx.accounts.ad_campaign_bid_tracks.key(), changes_count: campaign_changes_count, index, earned: 0 });

            let byted: Vec<u8> = updates.try_to_vec()?;
            ref_data[26..26+byted.len()].copy_from_slice(&byted);

        }        

        Ok(())
    }

    pub fn bid_on_space(ctx: Context<BidOnSpace>,index:u32, bid_config:BidConfig, messenger_slot:u64, asset_bundler:AssetBundler) -> Result<()> {

        let store = &mut ctx.accounts.store;
        

        msg!("messenger_slot {:?}",messenger_slot);

        let realloc_happens_on_er = true;

        let ad_campaign: &mut UncheckedAccount<'_> = &mut ctx.accounts.ad_campaign;
        let ad_campaign_key = ad_campaign.key();
        let ad_spot = &mut ctx.accounts.ad_spot;

        ad_spot.live_bids = ad_spot.live_bids+1;
        

        let system_program_info = ctx.accounts.system_program.to_account_info();
        let creator_info = ctx.accounts.creator.to_account_info();

        let mut different_size = 0;
        let (bloom_bits, k) = optimal_bloom_parameters( bid_config.max_viewers as usize * 10, 1.0 / bid_config.bloom_accuracy as f64);
        let bytes_in_bloom:usize = (bloom_bits / 8) * 2;

        let max_space = 10240 - START_FROM_BID_TRACK;
        msg!("bloom {:?} <= {:?}",bytes_in_bloom, max_space);
        msg!("acc {:?} viewerd {:?}",bid_config.bloom_accuracy, bid_config.max_viewers);
        let must_delegate = if bytes_in_bloom <= max_space {
            true
        } else {
            false
        };

        let max_space = 10240 - START_FROM_BID_TRACK;
        let extra_bytes = if bytes_in_bloom > max_space {
            max_space
        } else {
            bytes_in_bloom
        };

        let bytes_to_resize = START_FROM_BID_TRACK + if realloc_happens_on_er { 0 } else { extra_bytes }; 

        {
            let bid_track = &mut ctx.accounts.bid_track;
            let rent = Rent::get()?;
            let min_base = rent.minimum_balance(bytes_to_resize);
            let bid_track_info = bid_track.to_account_info();
            //bid_track_info.resize(bytes_to_resize)?;
            let bid_track_bump = ctx.bumps.bid_track.to_le_bytes();
            let created = create_account(
                CpiContext::new_with_signer(
                system_program_info.clone(),
                CreateAccount {
                    from: creator_info.clone(),
                    to: bid_track_info.clone()
                },
                &[&[
                    b"bid_track".as_ref(),
                    ad_campaign_key.as_ref(),
                    &index.to_le_bytes(),
                    &bid_track_bump
                ]]
                ),
                min_base,
                bytes_to_resize as u64,
                &crate::ID
            );
            match created {
                    Ok(()) => {
                    }
                    Err(err) => {
                        return Err(ProgramError::InvalidAccountOwner.into())
                    }
            }

        }

        let mut campaign_changes_count:u64 = 0;
        let (need_to_pay_rent, total_needed) = {

            let bid_track = &ctx.accounts.bid_track;

            let bid_track_info = bid_track.to_account_info();

            let ad_campaign_info = ad_campaign.to_account_info();

            let mut ref_data = ad_campaign_info.try_borrow_mut_data()?;

            let spaces_bids = u32::from_le_bytes(ref_data[TO_AD_CAMPAIGN_SPACES_BID..TO_AD_CAMPAIGN_SPACES_BID+4].try_into().unwrap());

            if realloc_happens_on_er || must_delegate {
                campaign_changes_count = u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_CHANGE_COUNT..TO_AD_CAMPAIGN_CHANGE_COUNT+8].try_into().unwrap()) + 1;
            }

            let currency_bytes:[u8;32] = ref_data[TO_AD_CAMPAIGN_CONFIG_CURRENCY..TO_AD_CAMPAIGN_CONFIG_CURRENCY+32].try_into().unwrap();
            let currency = Pubkey::new_from_array(currency_bytes);

            if currency_bytes == [0;32] {

                let bid_track_key = bid_track_info.key();
                let creator_key = ctx.accounts.creator.key();

                let ix = anchor_lang::solana_program::system_instruction::transfer( &creator_key, &bid_track_key,  bid_config.budget); 
                let send = anchor_lang::solana_program::program::invoke( &ix, &[ ctx.accounts.creator.to_account_info(), bid_track_info ]);
                
                match send {
                    Ok(()) => {
                        msg!("gp");
                    }
                    Err(_err) => {
                        return Err(ProgramError::InvalidArgument.into())
                    }
                } 

            } 

            if realloc_happens_on_er || must_delegate {
                ref_data[TO_AD_CAMPAIGN_CHANGE_COUNT..TO_AD_CAMPAIGN_CHANGE_COUNT+8].copy_from_slice(&(campaign_changes_count).to_le_bytes());
            }

            let max_space_needed = if spaces_bids > index { spaces_bids } else { index+1 } + 1;

            different_size = max_space_needed - spaces_bids;

            let total_needed = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * max_space_needed as usize;

            let has_bytes_now = ref_data.len();
            let need_to_pay_rent = if total_needed > has_bytes_now {

                let rent = Rent::get()?;
                let needs_to_cover = rent.minimum_balance(total_needed);
                let should_have_now = rent.minimum_balance(has_bytes_now);
  
                needs_to_cover - should_have_now

            } else {
                0
            };

            let space_pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * index as usize;
            if ref_data[space_pos] != 0 {
                msg!("already used");
                return Err(ProgramError::InvalidArgument.into())
            }

            ref_data[space_pos] = AD_CAMPAIGN_BID_STATE_CREATING;

            if max_space_needed > spaces_bids {
                ref_data[TO_AD_CAMPAIGN_SPACES_BID..TO_AD_CAMPAIGN_SPACES_BID+4].copy_from_slice(&max_space_needed.to_le_bytes());
            }

            let living_bids = u32::from_le_bytes(ref_data[TO_AD_CAMPAIGN_LIVING_BID..TO_AD_CAMPAIGN_LIVING_BID+4].try_into().unwrap());
            ref_data[TO_AD_CAMPAIGN_LIVING_BID..TO_AD_CAMPAIGN_LIVING_BID+4].copy_from_slice(&(living_bids+1).to_le_bytes());

            (need_to_pay_rent, total_needed)
         };

         if need_to_pay_rent > 0 {
                          
            let ad_campaign_info = ad_campaign.to_account_info();
            let result = ad_campaign_info.resize(total_needed);
            match result {
                    Ok(()) => {
                    }
                    Err(err) => {
                        msg!("VA {:?}",err);
                        return Err(ProgramError::InvalidRealloc.into())
                    }
            }

         }

         if need_to_pay_rent > 0 {

            let ad_campaign_info = ctx.accounts.ad_campaign.to_account_info();

            let ad_campaign_key = ad_campaign_info.key();
            let creator_key = ctx.accounts.creator.key();

            let ix = anchor_lang::solana_program::system_instruction::transfer( &creator_key, &ad_campaign_key,  need_to_pay_rent); 
            let send = anchor_lang::solana_program::program::invoke( &ix, &[ ctx.accounts.creator.to_account_info(), ad_campaign_info ]);
            
            match send {
                Ok(()) => {
                    msg!("gp");
                }
                Err(_err) => {
                    return Err(ProgramError::InvalidArgument.into())
                }
            } 

         }
        
        {

            let bid_track = &mut ctx.accounts.bid_track;

            let total_amount_to_pay = bytes_in_bloom+START_FROM_BID_TRACK;

            let rent = Rent::get()?;
            
            let total_base = rent.minimum_balance(total_amount_to_pay);
            let bid_track_info = bid_track.to_account_info();
            let prev_base = rent.minimum_balance(bid_track_info.data_len());

            let needs_to_pay = if total_base > prev_base { total_base - prev_base } else { 0 };

            if needs_to_pay > 0 {


                let bid_track_key = bid_track_info.key();
                let creator_key = ctx.accounts.creator.key();

                let ix = anchor_lang::solana_program::system_instruction::transfer( &creator_key, &bid_track_key,  needs_to_pay); 
                let send = anchor_lang::solana_program::program::invoke( &ix, &[ ctx.accounts.creator.to_account_info(), bid_track_info ]);
                
                match send {
                    Ok(()) => {
                        msg!("gp");
                    }
                    Err(_err) => {
                        return Err(ProgramError::InvalidArgument.into())
                    }
                } 

            }
            
        }


        {

           
            let bid_track = &mut ctx.accounts.bid_track;
            let bid_track_info = bid_track.to_account_info();
            

            let mut ref_data = bid_track_info.try_borrow_mut_data()?;

            ref_data[0..8].copy_from_slice(&BID_TRACK_DISCRIMINATOR);
            ref_data[8] = AccountClass::AdTrackV1 as u8;
            ref_data[9] = 0;
            ref_data[10] = 0;
            ref_data[TO_BID_TRACK_AD_CAMPAIGN..TO_BID_TRACK_AD_CAMPAIGN+32].copy_from_slice(&ad_campaign_key.to_bytes());
            ref_data[TO_BID_TRACK_AD_SPOT..TO_BID_TRACK_AD_SPOT+32].copy_from_slice(&ad_spot.key().to_bytes());
            ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].copy_from_slice(&index.to_le_bytes());
            ref_data[TO_BID_TRACK_AD_CONFIG..TO_BID_TRACK_AD_CONFIG+BID_TRACK_CONFIG_SIZE].copy_from_slice(&bid_config.try_to_vec()?);


            ref_data[TO_BID_TRACK_SIZE_HELPER..TO_BID_TRACK_SIZE_HELPER+4].copy_from_slice(&(bytes_in_bloom as u32).to_le_bytes());

            if must_delegate {
                ref_data[9] = 1;
                ref_data[10] = 1;
            }

        }

        let ad_campaign_bid_tracks = &ctx.accounts.ad_campaign_bid_tracks;
        let ad_campaign_bid_tracks_info = &ad_campaign_bid_tracks.to_account_info();

        if ad_campaign_bid_tracks_info.owner != &ctx.accounts.delegation_program.key() {
            msg!("ad bid track must be in er");
            return Err(ProgramError::InvalidAccountOwner.into())
        }

        let store_hash_bytes = store.store_hash.to_le_bytes();
        let bid_track_key = ctx.accounts.bid_track.key();
        
        {

            
            

            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();
            let slot_bytes = &messenger_slot.to_le_bytes();
            let bid_messenger_bump = ctx.bumps.bid_messenger.to_le_bytes();

            

            let bytes = 8 + MIN_ROLLUP_MESSENGER + SPACE_IN_CAMPAIGN_BID_TRACK * different_size as usize;
            let rent = Rent::get()?;
            let min_base = rent.minimum_balance(bytes);

            let real_bytes = 8 + MIN_ROLLUP_MESSENGER;
            
            let created = create_account(
                CpiContext::new_with_signer(
                system_program_info.clone(),
                CreateAccount {
                    from: creator_info.clone(),
                    to: bid_messenger_info.clone()
                },
                &[&[
                    b"rollup_bid_messenger".as_ref(),
                    bid_track_key.as_ref(),
                    &bid_messenger_bump
                ]]
                ),
                min_base,
                real_bytes as u64,
                &crate::ID
            );
            match created {
                    Ok(()) => {
                    }
                    Err(err) => {
                        return Err(ProgramError::InvalidAccountOwner.into())
                    }
            }

            let creator_key = ctx.accounts.creator.key();
            let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;

            ref_data[0..8].copy_from_slice(&ROLLUP_MESSENGER_DISCRIMINATOR);
            ref_data[8] = AccountClass::RollupMessengerV1 as u8;
            ref_data[9..17].copy_from_slice(&store.store_hash.to_le_bytes());
            ref_data[MESSENGER_TO_CREATOR..MESSENGER_TO_CREATOR+32].copy_from_slice(&creator_key.as_ref());
            ref_data[MESSENGER_TO_STATE] = if realloc_happens_on_er || must_delegate { MESSENGER_DELEGATED } else { MESSENGER_PENDING_DELEGATION };
            ref_data[MESSENGER_TO_SLOT..MESSENGER_TO_SLOT+8].copy_from_slice(&messenger_slot.to_le_bytes());

            let mut updates:Vec<RollupUpdates> = vec![];

            updates.push(RollupUpdates::RegisterBid { account: ctx.accounts.ad_campaign_bid_tracks.key(), changes_count: campaign_changes_count, index, budget: bid_config.budget, budget_per_click: bid_config.budget_per_click, budget_per_view: bid_config.budget_per_view });

            let byted: Vec<u8> = updates.try_to_vec()?;
            ref_data[26..26+byted.len()].copy_from_slice(&byted);
            
        }

        let mut creators = vec![];

        let creator_key = ctx.accounts.creator.key();

        let (ad_creator_key,bump) = Pubkey::find_program_address(&[b"ad_creator", creator_key.as_ref(), &store.store_hash.to_le_bytes()], &crate::ID);

        creators.push(Creator {
            address:creator_key,
            share:100,
            verified:false
        });
        
        creators.push(Creator {
            address:ad_creator_key,
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:ctx.accounts.bid_track.key(),
            share:0,
            verified:false
        });

        creators.push(Creator {
            address:ctx.accounts.ad_campaign.key(),
            share:0,
            verified:false
        });
        

        let clock = Clock::get().unwrap();
        let clock32 = clock.unix_timestamp.clamp(0, u32::MAX as i64) as u32;

        let mut payload:Vec<u8> = vec![];
        payload.push(0);
        payload.extend(clock32.to_le_bytes());

        let mut uri = match asset_bundler {
            AssetBundler::Arweave => {
                "https://arweave.net/6dGCqBJqOw-cEaFupbtLgZ-Mge1yWHKIW4NhzGS5F70"
            }
            AssetBundler::IrysGateway => {
                "https://gateway.irys.xyz/6dGCqBJqOw-cEaFupbtLgZ-Mge1yWHKIW4NhzGS5F70"
            }
            _ => {
                "https://arweave.net/6dGCqBJqOw-cEaFupbtLgZ-Mge1yWHKIW4NhzGS5F70"
            }
        }.to_string();
        
        
        if payload.len() > 0 {
            uri += "?p=";
            uri += encode_bytes_to_ascii_string(&payload, true).as_str();
        }

        let metadata = MetadataArgs {
            name:"Bid V0".to_string(),
            symbol:encode_u64_to_ascii_string(index as u64),
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


        let bid_track_info = ctx.accounts.bid_track.to_account_info();
        

        let result = mint_to_collection_cnft(
        &bubblegum_program_info,
        &tree_authority_info,
        &bid_track_info,
        &bid_track_info,
        &merkle_tree_info,
        &ctx.accounts.creator.to_account_info(),
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

        
        if realloc_happens_on_er || must_delegate {

             {
                let seed_1 = b"rollup_bid_messenger";
                let seed_2 = &bid_track_key.to_bytes();
                //let seed_3 = &messenger_slot.to_le_bytes();
                let pda_seeds: &[&[u8]] = &[seed_1, seed_2];
                ctx.accounts.delegate_bid_messenger(
                    &ctx.accounts.creator,
                    pda_seeds,
                    DelegateConfig {
                        validator: Some(Pubkey::new_from_array(DEVNET_VALIDATOR)),
                        ..Default::default()
                    },
                )?;
            }

            {

                //let ad_spot_key = ctx.accounts.ad_spot.key();

                let seed_1 = b"bid_track";
                let seed_2 = ad_campaign_key.as_ref();
                let seed_3 = &index.to_le_bytes();
                let pda_seeds: &[&[u8]] = &[seed_1, seed_2, seed_3];

                ctx.accounts.delegate_bid_track(
                    &ctx.accounts.creator,
                    pda_seeds,
                    DelegateConfig {
                        validator: Some(Pubkey::new_from_array(DEVNET_VALIDATOR)),
                        ..Default::default()
                    },
                )?;
            }

        }

        

      
        Ok(())
    }

    pub fn close_messenger_bid_track(ctx: Context<CloseMessengerBidTrack>) -> Result<()> {

        let bid_messenger = &ctx.accounts.bid_messenger;

        let creator_key = ctx.accounts.creator.key();

        

        let index = match bid_messenger.needs_to_update[0] {

            RollupUpdates::RegisterBid { account, changes_count, index, budget, budget_per_click, budget_per_view } => {

                if ctx.accounts.ad_campaign_bid_tracks.key() != account {
                    return Err(ProgramError::InvalidAccountData.into())
                }

                index
            }
            _ => {
                 return Err(ProgramError::InvalidAccountData.into())
            }
        };

        {
            let bid_track_info = ctx.accounts.bid_track.to_account_info();
            let ref_data = bid_track_info.try_borrow_data()?;

            let ad_spot = Pubkey::new_from_array(ref_data[TO_BID_TRACK_AD_SPOT..TO_BID_TRACK_AD_SPOT+32].try_into().unwrap());

            let compare_key = ctx.accounts.ad_spot.key();
            if ad_spot != compare_key {
                msg!("bad data {:?} {:?}",ad_spot,compare_key);
                return Err(ProgramError::InvalidAccountData.into())
            }
        }

        {
            let ad_campaign_info = ctx.accounts.ad_campaign.to_account_info();

            let mut ref_data = ad_campaign_info.try_borrow_mut_data()?;

            let space_pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * index as usize;
            if ref_data[space_pos] != AD_CAMPAIGN_BID_STATE_CREATING {
                msg!("bad step {:?}",ref_data[space_pos]);
                return Err(ProgramError::InvalidArgument.into())
            }
            ref_data[space_pos] = AD_CAMPAIGN_BID_STATE_READY;

        }

        {

            let bid_messenger_creator_info = ctx.accounts.messenger_creator.to_account_info();
            let bid_messenger_info = bid_messenger.to_account_info();

            let total_lamports_in_messenger = bid_messenger_info.lamports();
            let get_paid = 5000+10000;
            let remaining_lamports = total_lamports_in_messenger - get_paid;

            if bid_messenger.creator != creator_key {
                pay_to_user(&bid_messenger_info, &bid_messenger_creator_info, remaining_lamports)?;
            }

        }

        Ok(())
    }

    pub fn record_view_click(ctx: Context<RecordViewClick>,index:u32, view:u8, click:u8, identifier:Pubkey) -> Result<()> {

        let ad_campaign = &ctx.accounts.ad_campaign;

        let creator_key = ctx.accounts.creator.key();

        let store = &ctx.accounts.store;

        if store.master_manager != creator_key {
            return Err(ProgramError::IllegalOwner.into());
        }
        

       {
        let ad_campaign_info = ad_campaign.to_account_info();
        let ref_data_ad_campaign = ad_campaign_info.try_borrow_data()?;

        let pos = START_FROM_AD_CAMPAIGN + SPACE_IN_CAMPAIGN_BID * index as usize;
        if ref_data_ad_campaign[pos] != AD_CAMPAIGN_BID_STATE_READY {
            return Err(ProgramError::InvalidAccountData.into());
        }

        let ad_campaign_bid_track = &mut ctx.accounts.ad_campaign_bid_tracks;

        let ad_campaign_bid_track_info = ad_campaign_bid_track.to_account_info();
        let mut ref_data_ad_campaign_bid_track = ad_campaign_bid_track_info.try_borrow_mut_data()?;

        let pos_campaign_bid = START_FROM_AD_CAMPAIGN_BID_TRACKS + SPACE_IN_CAMPAIGN_BID_TRACK * index as usize;
        if ref_data_ad_campaign_bid_track[pos_campaign_bid] != 1 {
            return Err(ProgramError::InvalidAccountData.into());
        }


        let bid_track = &mut ctx.accounts.bid_track;
        let bid_track_info = bid_track.to_account_info();

        let mut ref_data_bid_track = bid_track_info.try_borrow_mut_data()?;
        
        let bid_config = BidConfig::try_from_slice(&ref_data_bid_track[TO_BID_TRACK_AD_CONFIG..TO_BID_TRACK_AD_CONFIG+BID_TRACK_CONFIG_SIZE])?;

        let mut total_views = u64::from_le_bytes(ref_data_bid_track[TO_BID_TRACK_TOTAL_VIEWS..TO_BID_TRACK_TOTAL_VIEWS+8].try_into().unwrap());
        
        let mut total_clicks = u64::from_le_bytes(ref_data_bid_track[TO_BID_TRACK_TOTAL_CLICKS..TO_BID_TRACK_TOTAL_CLICKS+8].try_into().unwrap());

        let clock = Clock::get().unwrap();
        let clock32 = clock.unix_timestamp.clamp(0, u32::MAX as i64) as u32;
        let mut earned_money = 0;

        let (bloom_bits, k) = optimal_bloom_parameters( bid_config.max_viewers as usize * 10, 1.0 / bid_config.bloom_accuracy as f64);

        if click > 0 {

            let click_bloom_start = START_FROM_BID_TRACK+START_FROM_BID_TRACK;

            let mut not_contained = 0;
            for i in 0..BLOOM_HASH_COUNT {
                let index = hash(&identifier.to_bytes(), i as u64) % bloom_bits;
                let byte = index / 8;
                let bit = index % 8;
                let index_byte = click_bloom_start + byte;
                let exists = ref_data_bid_track[index_byte] & (1 << bit) != 0;
                if !exists { //
                    not_contained += 1;
                }
                ref_data_bid_track[index_byte] |= 1 << bit; //Ir guardando los bits
            }
            if not_contained > 0 {
                msg!("add click");
                total_clicks += 1;
                ref_data_bid_track[TO_BID_TRACK_TOTAL_CLICKS..TO_BID_TRACK_TOTAL_CLICKS+8].copy_from_slice(&total_clicks.to_le_bytes());
                earned_money += bid_config.budget_per_view;
            }

        }

        let last_reset = u32::from_le_bytes(ref_data_bid_track[TO_BID_TRACK_TOTAL_VIEWS..TO_BID_TRACK_TOTAL_VIEWS+4].try_into().unwrap());

        let views_in_day = u32::from_le_bytes(ref_data_bid_track[TO_BID_TRACK_VIEWS_IN_DAY..TO_BID_TRACK_VIEWS_IN_DAY+4].try_into().unwrap());

        if view > 0 {

            let view_bloom_start = START_FROM_BID_TRACK;

            let mut not_contained = 0;
            for i in 0..BLOOM_HASH_COUNT {
                let index = hash(&identifier.to_bytes(), i as u64) % bloom_bits;
                let byte = index / 8;
                let bit = index % 8;
                let index_byte = view_bloom_start + byte;
                let exists = ref_data_bid_track[index_byte] & (1 << bit) != 0;
                if !exists { //
                    not_contained += 1;
                }
                ref_data_bid_track[index_byte] |= 1 << bit; //Ir guardando los bits
            }

            if not_contained > 0 {
                msg!("add view");
                total_views += 1;
                ref_data_bid_track[TO_BID_TRACK_TOTAL_VIEWS..TO_BID_TRACK_TOTAL_VIEWS+8].copy_from_slice(&total_views.to_le_bytes());

                earned_money += bid_config.budget_per_click;

                ref_data_bid_track[TO_BID_TRACK_VIEWS_IN_DAY..TO_BID_TRACK_VIEWS_IN_DAY+4].copy_from_slice(&(views_in_day+1).to_le_bytes());
            }
        }

        
        
        let spent_budget = total_views * bid_config.budget_per_view + total_clicks * bid_config.budget_per_click;

        //budget has been spent
        if spent_budget >= bid_config.budget {
            ref_data_ad_campaign_bid_track[pos_campaign_bid+1] = 0;
        }

        if earned_money > 0 {

            let pos_to_last_view:usize = 18;
            ref_data_bid_track[pos_campaign_bid+pos_to_last_view..pos_campaign_bid+pos_to_last_view+4].copy_from_slice(&clock32.to_le_bytes());

            let earned = u64::from_le_bytes(ref_data_ad_campaign_bid_track[TO_AD_CAMPAIGN_BID_EARNED..TO_AD_CAMPAIGN_BID_EARNED+8].try_into().unwrap());
            
            ref_data_ad_campaign_bid_track[TO_AD_CAMPAIGN_BID_EARNED..TO_AD_CAMPAIGN_BID_EARNED+8].copy_from_slice(&(earned+earned_money).to_le_bytes());

        }



       }

       Ok(())


    }

     pub fn grow_ad_track(ctx: Context<GrowAdTrack>) -> Result<()> {

        let bid_messenger = &mut ctx.accounts.bid_messenger;
        
        let store = &ctx.accounts.store;
        let creator_key = ctx.accounts.creator.key();
        let manager = store.master_manager;

        let ad_creator = &ctx.accounts.ad_creator;

        if creator_key != manager && creator_key != ad_creator.creator && ad_creator.creator_manager != creator_key {
            return Err(ProgramError::InvalidAccountData.into())
        }

        let (first_time, needs_to_update) = {

            let bid_messenger_info = bid_messenger.to_account_info();
            let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;
            if ref_data[MESSENGER_TO_STATE] == MESSENGER_DELEGATED {
                ref_data[MESSENGER_TO_STATE] = MESSENGER_DELEGATED_PROCESSING;

                (true, RollupUpdates::try_from_slice(&ref_data[MESSENGER_TO_PAYLOAD..MESSENGER_TO_PAYLOAD+MESSAGE_PAYLOAD_SIZE])?)
            } else if ref_data[MESSENGER_TO_STATE] == MESSENGER_DELEGATED_PROCESSING {
                (false, RollupUpdates::try_from_slice(&ref_data[MESSENGER_TO_PAYLOAD..MESSENGER_TO_PAYLOAD+MESSAGE_PAYLOAD_SIZE])?)
            } else if ref_data[MESSENGER_TO_STATE] == MESSENGER_DELEGATED_PROCESSED {
                return Ok(());
            } else {
                return Err(ProgramError::InvalidAccountData.into());
            }

        };
        

        let (account,bid_config, index, changes_count) = match needs_to_update {
            RollupUpdates::RegisterBid { account, changes_count, index, budget, budget_per_click, budget_per_view } => {
                let (max_viewers, bloom_accuracy) = {
                    let info = ctx.accounts.bid_track.to_account_info();
                    let ref_data = info.try_borrow_data()?;
                    (
                        u16::from_le_bytes(ref_data[TO_BID_TRACK_MAX_VIEWERS..TO_BID_TRACK_MAX_VIEWERS+2].try_into().unwrap()),
                        u16::from_le_bytes(ref_data[TO_BID_TRACK_BLOOM_ACCURACY..TO_BID_TRACK_BLOOM_ACCURACY+2].try_into().unwrap())
                    )
                };
                (account, BidConfig { budget, budget_per_click, budget_per_view, bloom_accuracy, max_viewers }, index, changes_count)
                
            }
            _=>{
                 return Err(ProgramError::InvalidAccountData.into())
            }
        };

        if account != ctx.accounts.ad_campaign_bid_tracks.key() {
            return Err(ProgramError::InvalidAccountData.into())
        }
        
        let bid_data:Option<AdCampaignBidTrackPieceStruct> = if first_time {
            
            if account != ctx.accounts.ad_campaign_bid_tracks.key() {
                return Err(ProgramError::InvalidAccountData.into())
            }

            let min_bytes = 8 + MIN_ROLLUP_MESSENGER;
            let rent = Rent::get()?;
            let min_base = rent.minimum_balance(min_bytes);

            let info_messenger = bid_messenger.to_account_info();
            let now_lamports = info_messenger.lamports();

            let dif_money = now_lamports - min_base;

            let info_campaign_bid_tracks = ctx.accounts.ad_campaign_bid_tracks.to_account_info();

            //Add bytes to resize account
            if dif_money > 0 {
                pay_to_user(&info_messenger, &info_campaign_bid_tracks, dif_money)?;
            } 

            let bid_data = AdCampaignBidTrackPieceStruct {
                state:1,
                has_budget:1,
                budget_per_view:bid_config.budget_per_view,
                budget_per_click:bid_config.budget_per_click,
                last_view:0
            };
            Some(bid_data)

        } else {
            None
        };
        //Resize account
        let total_bytes = if bid_data.is_some() {
            let info_campaign_bid_tracks = ctx.accounts.ad_campaign_bid_tracks.to_account_info();
            let ref_data = info_campaign_bid_tracks.try_borrow_data()?;
            let spaces_bids = u32::from_le_bytes(ref_data[TO_AD_CAMPAIGN_BID_TRACK_SPACES..TO_AD_CAMPAIGN_BID_TRACK_SPACES+4].try_into().unwrap());
            let max_space_needed = if spaces_bids > index { spaces_bids } else { index+1 } + 1;
            START_FROM_AD_CAMPAIGN_BID_TRACKS + SPACE_IN_CAMPAIGN_BID_TRACK * max_space_needed as usize
        } else {
            0
        };
        if total_bytes > 0 {
            let info_campaign_bid_tracks = ctx.accounts.ad_campaign_bid_tracks.to_account_info();
            info_campaign_bid_tracks.resize(total_bytes)?;
        }

        //Put bid data in account
        if let Some(bid_data) = bid_data {
            let info_campaign_bid_tracks = ctx.accounts.ad_campaign_bid_tracks.to_account_info();
            let mut ref_data = info_campaign_bid_tracks.try_borrow_mut_data()?;
            let position = START_FROM_AD_CAMPAIGN_BID_TRACKS + SPACE_IN_CAMPAIGN_BID_TRACK * index as usize;
            if ref_data[position] != 0 {
                return Err(ProgramError::InvalidAccountData.into())
            }
            let vec_bytes = bid_data.try_to_vec()?;
            let size = vec_bytes.len();
            ref_data[position..position+size].copy_from_slice(&vec_bytes);

            msg!("vec_bytes {:?}",vec_bytes);

            let now_living = u32::from_le_bytes(ref_data[TO_AD_CAMPAIGN_BID_TRACK_LIVING..TO_AD_CAMPAIGN_BID_TRACK_LIVING+4].try_into().unwrap());
            let now_spaces = u32::from_le_bytes(ref_data[TO_AD_CAMPAIGN_BID_TRACK_SPACES..TO_AD_CAMPAIGN_BID_TRACK_SPACES+4].try_into().unwrap());

            if (index+1) > now_spaces {
                ref_data[TO_AD_CAMPAIGN_BID_TRACK_SPACES..TO_AD_CAMPAIGN_BID_TRACK_SPACES+4].copy_from_slice(&(index+1).to_le_bytes());    
            }

            ref_data[TO_AD_CAMPAIGN_BID_TRACK_LIVING..TO_AD_CAMPAIGN_BID_TRACK_LIVING+4].copy_from_slice(&(now_living+1).to_le_bytes());

            ref_data[TO_AD_CAMPAIGN_BID_TRACK_CHANGE_COUNT..TO_AD_CAMPAIGN_BID_TRACK_CHANGE_COUNT+8].copy_from_slice(&changes_count.to_le_bytes());
        }

        let bid_track_size = {
            let bid_track_info = &ctx.accounts.bid_track;
            bid_track_info.data_len()
        };

        let is_done = {

            let (bloom_bits, k) = optimal_bloom_parameters( bid_config.max_viewers as usize * 10, 1.0 / bid_config.bloom_accuracy as f64);
            let bytes_in_bloom:usize = (bloom_bits / 8) * 2;

            let bloom_space_now = bid_track_size - START_FROM_BID_TRACK;

            let (total_bloom_space, is_done) = if bytes_in_bloom > bloom_space_now {
                let faltan = bytes_in_bloom - bloom_space_now;
                if faltan > 10240 {
                    (bloom_space_now + 10240, false)
                } else {
                    (bloom_space_now + faltan, true)
                }
            } else {
                (bloom_space_now, true)
            };

            let total_possible_bytes = START_FROM_BID_TRACK + total_bloom_space;
            ctx.accounts.bid_track.to_account_info().resize(total_possible_bytes)?;
            if is_done {
                let bid_messenger_info = bid_messenger.to_account_info();
                let mut ref_data = bid_messenger_info.try_borrow_mut_data()?;
                ref_data[MESSENGER_TO_STATE] = MESSENGER_DELEGATED_PROCESSED;
            }
            is_done
        };

        if is_done {

            let bid_messenger_info = ctx.accounts.bid_messenger.to_account_info();

            let good = commit_and_undelegate_accounts(
                &ctx.accounts.creator.to_account_info(),
                vec![&bid_messenger_info],
                &ctx.accounts.magic_context,
                &ctx.accounts.magic_program,
            );

            match good {
                Ok(()) => {
                }
                Err(err) => {
                    // Handle error case  
                    msg!("err {:?}",err);
                    return Err(ProgramError::InvalidArgument.into())
                }
            } 

        }
        
            Ok(())
      }



}

const START_FROM_BID_TRACK:usize = 8+134;

const TO_BID_TRACK_BLOOM_ACCURACY:usize = 87;
const TO_BID_TRACK_MAX_VIEWERS:usize = 89;
const TO_BID_TRACK_AD_CAMPAIGN:usize = 11;
const TO_BID_TRACK_CAMPAIGN_BID_INDEX:usize = 43;
const TO_BID_TRACK_AD_SPOT:usize = 47;
const TO_BID_TRACK_AD_CONFIG:usize = 79;
const TO_BID_TRACK_SIZE_HELPER:usize = 138;

const BID_TRACK_CONFIG_SIZE:usize = 28;

const TO_BID_TRACK_TOTAL_VIEWS:usize = TO_BID_TRACK_AD_CONFIG+BID_TRACK_CONFIG_SIZE;
const TO_BID_TRACK_TOTAL_CLICKS:usize = TO_BID_TRACK_TOTAL_VIEWS+8;
const TO_BID_TRACK_VIEWS_IN_DAY:usize = TO_BID_TRACK_TOTAL_CLICKS+8;
const TO_BID_TRACK_LAST_RESET:usize = TO_BID_TRACK_TOTAL_CLICKS+8+4;


#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct BidConfig
{ //28
    pub budget: u64, //8
    pub bloom_accuracy: u16, //2
    pub max_viewers:u16, //2
    pub budget_per_view:u64, //8
    pub budget_per_click:u64 //8
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct AdTrackSetup
{ //134
    pub class: AccountClass, //1
    pub state: u8, //1
    pub status: u8, //1
    pub campaign: Pubkey, //32
    pub index:u32,//4
    pub spot: Pubkey, //32
    pub config:BidConfig, //28
    pub total_views:u64, //8
    pub total_clicks:u64, //8
    pub views_in_day:u32,//4
    pub last_reset:u32,//4
    pub updated_at:u32,//4
    pub needs_to_sync:u8, //1
    pub living_users:u16, //2
    pub size_helper:u32, //4
}


#[account]
//233
pub struct AdBidsSetup {
  pub class:AccountClass, //1
  pub campaign: Pubkey, //32
  pub owner:Pubkey, //8
  pub extra:[u8;53], //53
  pub live_bids:u16, //2
  pub live_spaces:u32 //4
}

const SPACE_IN_BIDS:usize = 80;
const TO_BID_VIEWS:usize = 32;
const TO_BID_CLICKS:usize = 40;
const TO_BID_SPENT:usize = 48;
const TO_BID_BUDGET:usize = 56;

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct Bid
{ //80
    pub creator: Pubkey, //32
    pub views: u64, //8
    pub clicks: u64, //8
    pub spent: u64, //8
    pub budget:u64, //8
    pub extra:[u8;16] //16
}


#[commit]
#[derive(Accounts)]
pub struct CommitDeleteBid<'info> {
    /// CHECK: unsafe
    #[account(
        seeds = [b"ad_campaign".as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,

    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref()],
        bump
    )]
    pub bid_messenger: UncheckedAccount<'info>,

    pub store: Box<Account<'info, Store>>,
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,

}

#[commit]
#[derive(Accounts)]
pub struct CommitCashout<'info> {
    /// CHECK: unsafe
    #[account(
        seeds = [b"ad_campaign".as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,

    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref()],
        bump
    )]
    pub bid_messenger: UncheckedAccount<'info>,

    pub store: Box<Account<'info, Store>>,
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,

}

#[derive(Accounts)]
pub struct CloseCommitCashout<'info> {
    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign".as_ref(), space_creator_owner.key().as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    #[account(
        seeds = [b"ad_spot".as_ref(), ad_spot.creator.as_ref(), &store.store_hash.to_le_bytes(), &ad_spot.slot.to_le_bytes()],
        bump
    )]
    pub ad_spot: Box<Account<'info, AdSpot>>,

    #[account(
        mut,
        seeds = [b"space_creator".as_ref(), space_creator_owner.key().as_ref(), &store.store_hash.to_le_bytes().as_ref()],
        bump
    )]
    pub space_creator: Box<Account<'info, SpaceCreator>>,
     /// CHECK: unsafe
    #[account(mut)]
    pub space_creator_owner: UncheckedAccount<'info>,
     /// CHECK: unsafe
    #[account(mut)]
    pub messenger_creator: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref(), space_creator_owner.key().as_ref()],
        bump,
        close = creator
    )]
    pub bid_messenger: Box<Account<'info, RollupMessenger>>,

    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,

}

#[derive(Accounts)]
pub struct CloseCommitDeleteBid<'info> {
    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign".as_ref(), space_creator_owner.key().as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"ad_spot".as_ref(), ad_creator_owner.key().as_ref(), &store.store_hash.to_le_bytes(), &ad_spot.slot.to_le_bytes()],
        bump
    )]
    pub ad_spot: Box<Account<'info, AdSpot>>,

    #[account(
        mut,
        seeds = [b"space_creator".as_ref(), space_creator_owner.key().as_ref(), &store.store_hash.to_le_bytes().as_ref()],
        bump
    )]
    pub space_creator: Box<Account<'info, SpaceCreator>>,

     /// CHECK: unsafe
    #[account(mut)]
    pub space_creator_owner: UncheckedAccount<'info>,
     /// CHECK: unsafe
    #[account(mut)]
    pub messenger_creator: UncheckedAccount<'info>,
     /// CHECK: unsafe
    #[account(mut)]
    pub ad_creator_owner: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref()],
        bump,
        close = creator
    )]
    pub bid_messenger: Box<Account<'info, RollupMessenger>>,

    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,

}

#[delegate]
#[derive(Accounts)]
pub struct CashoutEarnings<'info> {
    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign".as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,


    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        del,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref()],
        bump
    )]
    pub bid_messenger: UncheckedAccount<'info>,

    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,

}

#[derive(Accounts)]
pub struct CloseMessengerBidTrack<'info> {
    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign".as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,

    #[account(
        seeds = [b"ad_spot".as_ref(), ad_spot_creator.key().as_ref(), &store.store_hash.to_le_bytes(), &ad_spot.slot.to_le_bytes()],
        bump
    )]
    pub ad_spot: Box<Account<'info, AdSpot>>,

    /// CHECK: unsafe
     #[account(mut)]
    pub ad_spot_creator: UncheckedAccount<'info>,

    /// CHECK: unsafe
     #[account(mut)]
    pub messenger_creator: UncheckedAccount<'info>,

    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref()],
        bump,
        close = creator
    )]
    pub bid_messenger: Box<Account<'info, RollupMessenger>>,

    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[commit]
#[derive(Accounts)]
pub struct GrowAdTrack<'info> {
    /// CHECK: unsafe
    #[account(
        seeds = [b"ad_campaign".as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,

    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref()],
        bump
    )]
    pub bid_messenger: UncheckedAccount<'info>,

    #[account(
        seeds = [b"ad_creator".as_ref(), ad_creator.creator.key().as_ref(), &store.store_hash.to_le_bytes()],
        bump
    )]
    pub ad_creator: Box<Account<'info, AdCreator>>,
    pub store: Box<Account<'info, Store>>,
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
#[instruction(index:u32, view:u8, click:u8, identifier:Pubkey)]
pub struct RecordViewClick<'info> {

    /// CHECK: unsafe
    #[account(
        seeds = [b"ad_campaign".as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,

    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    pub store: Box<Account<'info, Store>>,
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

const MIN_ROLLUP_MESSENGER:usize = 123;

#[delegate]
#[derive(Accounts)]
pub struct DeleteBid<'info> {

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign".as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,


    /// CHECK: unsafe
     #[account(
        mut,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &{
            let info = bid_track.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u32::from_le_bytes(ref_data[TO_BID_TRACK_CAMPAIGN_BID_INDEX..TO_BID_TRACK_CAMPAIGN_BID_INDEX+4].try_into().unwrap())
        }.to_le_bytes()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        del,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref()],
        bump
    )]
    pub bid_messenger: UncheckedAccount<'info>,

    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
    
}

#[delegate]
#[derive(Accounts)]
#[instruction(index:u32, bid_config:BidConfig, messenger_slot:u64, asset_bundler:AssetBundler)]
pub struct BidOnSpace<'info> {
    
    /// CHECK: unsafe
     #[account(
        mut,
        del,
        seeds = [b"bid_track".as_ref(), ad_campaign.key().as_ref(), &index.to_le_bytes().as_ref()],//ad_spot.key().as_ref()],
        bump
    )]
    pub bid_track: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign_bid_tracks".as_ref(), ad_campaign.key().as_ref()],
        bump,
    )]
    pub ad_campaign_bid_tracks: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        del,
        seeds = [b"rollup_bid_messenger".as_ref(), bid_track.key().as_ref()],
        bump
    )]
    pub bid_messenger: UncheckedAccount<'info>,

    /// CHECK: unsafe
    #[account(
        mut,
        seeds = [b"ad_campaign".as_ref(), {
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            Pubkey::new_from_array(ref_data[TO_AD_CAMPAIGN_CREATOR..TO_AD_CAMPAIGN_CREATOR+32].try_into().unwrap())
        }.as_ref(), &{
            let info = ad_campaign.to_account_info();
            let ref_data = info.try_borrow_data()?;
            u64::from_le_bytes(ref_data[TO_AD_CAMPAIGN_STORE_HASH..TO_AD_CAMPAIGN_STORE_HASH+8].try_into().unwrap())
        }.to_le_bytes().as_ref(),{
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
    pub ad_campaign: UncheckedAccount<'info>, 
    #[account(
        mut,
        seeds = [b"ad_spot".as_ref(), creator.key().as_ref(), &store.store_hash.to_le_bytes().as_ref(), &ad_spot.slot.to_le_bytes()],
        bump
    )]
    pub ad_spot: Box<Account<'info, AdSpot>>,


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



    #[account(mut)]
    pub store: Box<Account<'info, Store>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]

pub struct RollupMessenger {
  pub class:AccountClass, //1
  pub store_hash:u64, //8
  pub state:u8, //1
  pub creator:Pubkey, //32
  pub slot:u64, //8
  pub needs_to_update:Vec<RollupUpdates> //4+69
}



const MESSENGER_DELEGATED:u8 = 1;
const MESSENGER_PENDING_DELEGATION:u8 = 2;
const MESSENGER_DELEGATED_PROCESSING:u8 = 3;
const MESSENGER_DELEGATED_PROCESSED:u8 = 4;

const MESSENGER_DELEGATED_CASHOUT:u8 = 5;
const MESSENGER_PROCESSED_CASHOUT:u8 = 6;


const MESSAGE_PAYLOAD_SIZE:usize = 69;
const MESSAGE_CASHOUT_PAYLOAD_SIZE:usize = 53;
const MESSAGE_DELETING_PAYLOAD_SIZE:usize = 53;

const MESSENGER_TO_PAYLOAD:usize = 62;
const MESSENGER_TO_CASHOUT_EARNED:usize = MESSENGER_TO_PAYLOAD+45;
const MESSENGER_TO_STATE:usize = 17;
const MESSENGER_TO_INDEX:usize = MESSENGER_TO_PAYLOAD+41;
const MESSENGER_TO_CREATOR:usize = 18;
const MESSENGER_TO_STORE_HASH:usize = 9;
const MESSENGER_TO_SLOT:usize = 50;
const MESSENGER_TO_CHANGES_COUNT:usize = MESSENGER_TO_PAYLOAD+33;

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub enum RollupUpdates {
  RegisterBid { //69
    account:Pubkey, //32
    changes_count:u64, //8
    index:u32, //4
    budget:u64, //8
    budget_per_click:u64, //8
    budget_per_view:u64, //8
  },
  CashoutBid { //53
    account:Pubkey, //32
    changes_count:u64, //8
    index:u32, //4
    earned:u64, //8
  },
  DeletingBid { //53
    account:Pubkey, //32
    changes_count:u64, //8
    index:u32, //4
    must_pay_user:u64, //8
  }
}

#[account(zero_copy)]
#[repr(C)]
pub struct AdBids {
  /*
  */
  pub data:[u8;10_000_000]
}

#[account(zero_copy)]
#[repr(C)]
pub struct BidTrack {
  /*
  */
  pub data:[u8;10_000_000]
}


  /*let ad_track = &mut ctx.accounts.ad_track;
        let ad_track_info = ad_track.to_account_info();

        let system_program_info = ctx.accounts.system_program.to_account_info();
        let creator_info = ctx.accounts.creator.to_account_info();

        let ad_campaign = &mut ctx.accounts.ad_campaign;
        let ad_campaign_key = ad_campaign.key();
        
        let ad_track_bump = &ctx.bumps.ad_track.to_le_bytes();
        //let ad_campaign_info = ad_campaign.to_account_info();

        {
            if ad_track_info.data_len() == 0 {

                let bytes_in_ad_track = START_FROM_AD_TRACK + SPACE_IN_AD_TRACK;

                let rent = Rent::get()?;
                let min_base = rent.minimum_balance(bytes_in_bloom);

                let created = create_account(
                    CpiContext::new_with_signer(
                    system_program_info.clone(),
                    CreateAccount {
                        from: creator_info.clone(),
                        to: ad_track_info.clone()
                    },
                    &[&[
                        b"ad_track".as_ref(),
                        ad_campaign_key.as_ref(),
                        ad_track_bump
                    ]]
                    ),
                    0,
                    0,
                    &crate::ID
                );

            }
        } */

        