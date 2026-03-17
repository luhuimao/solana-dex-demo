use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;

/// Byte size of a serialized Pool account
/// 1 (bool) + 5*32 (Pubkeys) + 5*8 (u64) + 1 (u8 bump) = 202
pub const POOL_SIZE: usize = 202;

/// Liquidity pool state stored in a dedicated account.
///
/// Seeds for the pool_authority PDA:
///   `["pool_authority", pool_account_key]`
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct Pool {
    /// Whether this pool has been initialized
    pub is_initialized: bool,

    /// Mint of token A (e.g., USDC)
    pub token_a_mint: Pubkey,
    /// Mint of token B (e.g., SOL-wrapped)
    pub token_b_mint: Pubkey,

    /// Pool's token A vault (owned by pool_authority PDA)
    pub token_a_vault: Pubkey,
    /// Pool's token B vault (owned by pool_authority PDA)
    pub token_b_vault: Pubkey,

    /// LP token mint (mint authority = pool_authority PDA)
    pub lp_mint: Pubkey,

    /// Current token A reserve (cached, must equal vault balance)
    pub reserve_a: u64,
    /// Current token B reserve (cached, must equal vault balance)
    pub reserve_b: u64,
    /// Total LP token supply outstanding
    pub lp_supply: u64,

    /// Fee numerator   (e.g., 3  → 0.3% fee)
    pub fee_numerator: u64,
    /// Fee denominator (e.g., 1000)
    pub fee_denominator: u64,

    /// Canonical bump seed for pool_authority PDA
    pub bump: u8,
}
