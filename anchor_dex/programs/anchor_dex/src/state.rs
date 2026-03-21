use anchor_lang::prelude::*;

/// Space = 8 (discriminator) + 5×32 (Pubkeys) + 5×8 (u64) + 1 (u8) = 209
pub const POOL_SPACE: usize = 8 + 5 * 32 + 5 * 8 + 1;

#[account]
#[derive(Debug)]
pub struct Pool {
    /// Mint of token A (e.g., USDC)
    pub token_a_mint: Pubkey,
    /// Mint of token B (e.g., wSOL)
    pub token_b_mint: Pubkey,

    /// Pool's token A vault (owned by pool_authority PDA)
    pub token_a_vault: Pubkey,
    /// Pool's token B vault (owned by pool_authority PDA)
    pub token_b_vault: Pubkey,

    /// LP token mint (mint authority = pool_authority PDA)
    pub lp_mint: Pubkey,

    /// Cached token A reserve (must equal vault balance)
    pub reserve_a: u64,
    /// Cached token B reserve (must equal vault balance)
    pub reserve_b: u64,
    /// Total LP token supply outstanding
    pub lp_supply: u64,

    /// Fee numerator   (e.g., 3  → 0.3%)
    pub fee_numerator: u64,
    /// Fee denominator (e.g., 1000)
    pub fee_denominator: u64,

    /// Canonical bump seed for pool_authority PDA
    pub bump: u8,
}
