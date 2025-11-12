use anchor_lang::prelude::*;
use ephemeral_rollups_sdk::anchor::{commit, delegate, ephemeral};

declare_id!("6AszQASdX67xtwpVVBSRsba3NFmh8FS3su6t152TXWgp");


use base::*;
mod base;
use ads::*;
use bids::*;
use cnft::{AssetBundler};
mod ads;
mod bids;
mod bloom;
mod utils;
mod cnft;
mod adscategories;

#[ephemeral]
#[program]
pub mod xhttp_ad {

    use super::*;

    pub fn commit_cashout(ctx: Context<CommitCashout>) -> Result<()> {
        bids::bids_ix::commit_cashout(ctx)
    }

    pub fn commit_delete_bid(ctx: Context<CommitDeleteBid>) -> Result<()> {
        bids::bids_ix::commit_delete_bid(ctx)
    }

    pub fn close_commit_cashout(ctx: Context<CloseCommitCashout>) -> Result<()> {
        bids::bids_ix::close_commit_cashout(ctx)
    }

    pub fn close_messenger_bid_track(ctx: Context<CloseMessengerBidTrack>) -> Result<()> {
        bids::bids_ix::close_messenger_bid_track(ctx)
    }

    pub fn cashout_earnings(ctx: Context<CashoutEarnings>) -> Result<()> {
        bids::bids_ix::cashout_earnings(ctx)
    }

    pub fn register_ad_creator(ctx: Context<RegisterAdCreator>) -> Result<()> {
        ads::ads_ix::register_ad_creator(ctx)
    }

    pub fn prepare_delete_ad_campaign(ctx: Context<PrepareDeleteAdCampaign>) -> Result<()> {
        ads::ads_ix::prepare_delete_ad_campaign(ctx)
    }

    pub fn record_view_click(ctx: Context<RecordViewClick>,index:u32, view:u8, click:u8, identifier:Pubkey) -> Result<()> {
        bids::bids_ix::record_view_click(ctx,index,view,click, identifier)
    }

    pub fn bid_on_space(ctx: Context<BidOnSpace>,index:u32, bid_config:BidConfig, messenger_slot:u64, asset_bundler:AssetBundler) -> Result<()> {
        bids::bids_ix::bid_on_space(ctx,index,bid_config,messenger_slot, asset_bundler)
    }

    pub fn grow_ad_track(ctx: Context<GrowAdTrack>) -> Result<()> {
        bids::bids_ix::grow_ad_track(ctx)
    }

     pub fn register_space_creator(ctx: Context<RegisterSpaceCreator>) -> Result<()> {
        ads::ads_ix::register_space_creator(ctx)
     }

    pub fn create_ad(ctx: Context<CreateAd>,slot:u64, ad_config:AdConfig) -> Result<()> {
        ads::ads_ix::create_ad(ctx,slot, ad_config)
    }

    pub fn register_ad_space(ctx: Context<RegisterAdSpace>,slot:u32) -> Result<()> {
        ads::ads_ix::register_ad_space(ctx,slot)
    }

    pub fn delete_ad_spot<'a, 'b, 'c, 'info>(ctx: Context<'a, 'b, 'c, 'info, DeleteAdSpot<'info>>, cnft: CampaignCnft) -> Result<()> {
        ads::ads_ix::delete_ad_spot(ctx,cnft)
    }

    pub fn register_ad_spot(ctx: Context<RegisterAdSpot>, slot:u32, asset_source:AssetSource) -> Result<()> {
        ads::ads_ix::register_ad_spot(ctx,slot,asset_source)
    }

    pub fn delete_ad_campaign<'a, 'b, 'c, 'info>(ctx: Context<'a, 'b, 'c, 'info, DeleteAdCampaign<'info>>, cnft: CampaignCnft) -> Result<()> {
        ads::ads_ix::delete_ad_campaign(ctx,cnft)
    }

    pub fn register_ad_campaign(ctx: Context<RegisterAdCampaign>,twitter_proof:[u8;64], slot:u32,campaign_config:CampaignConfig, twitter:[u8;32], asset_source:AssetSource) -> Result<()> {
        ads::ads_ix::register_ad_campaign(ctx,twitter_proof,slot,campaign_config,twitter,asset_source)
    }

    pub fn create_universe(ctx: Context<CreateUniverse>,slot:u16) -> Result<()> {
        base::base_ix::create_universe(ctx, slot)
    }

    pub fn create_store(ctx: Context<CreateStore>, slot: u16) -> Result<()> {
        base::base_ix::create_store(ctx, slot)
    }

    pub fn create_master(ctx: Context<CreateMaster>,slot:u16) -> Result<()> {
        base::base_ix::create_master(ctx, slot)
    }

    pub fn feed_global_tree(ctx: Context<FeedGlobalTree>, max_depth:u32, max_buffer_size:u32, public:bool) -> Result<()> {
        base::base_ix::feed_global_tree(ctx, max_depth, max_buffer_size, public)
    }

    pub fn create_collection<'a, 'b, 'c, 'info>(ctx: Context<'a, 'b, 'c, 'info, CreateCollection<'info>>, token_metadata:crate::cnft::TokenMetadata, vault_type:String) -> Result<()> {
        base::base_ix::create_collection(ctx,token_metadata,vault_type)
    }

    
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
#[repr(u8)]
pub enum AccountClass {
  UniverseV1 = 0,//0
  StoreV1 = 1,//1
  MasterV1 = 2,//2
  AdCreatorV1 = 3,//2
  AdV1 = 4,//2
  AdSpaceV1 = 5,//2
  AdStatsV1 = 6,//2
  AdCampaignV1 = 7,//2
  SpaceCreatorV1 = 8,//2
  AdSpotV1 = 9,//2
  AdBidsV1 = 10,//2
  AdTrackV1 = 11,//2
  AdCampaignBidTrackV1 = 12,
  RollupMessengerV1 = 13,
}