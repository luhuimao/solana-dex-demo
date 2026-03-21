use anchor_lang::prelude::*;

#[error_code]
pub enum DexError {
    #[msg("Fee denominator must be > 0 and fee numerator < denominator")]
    InvalidFee,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Insufficient liquidity in the pool")]
    InsufficientLiquidity,
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Invalid account: vault mint does not match pool mint")]
    InvalidVaultMint,
}
