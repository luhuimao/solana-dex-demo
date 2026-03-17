use solana_program::program_error::ProgramError;

/// DEX custom errors
#[derive(Debug, Clone, PartialEq)]
pub enum DexError {
    /// Pool is already initialized
    AlreadyInitialized = 0,
    /// Pool is not initialized
    NotInitialized = 1,
    /// Pool has insufficient liquidity
    InsufficientLiquidity = 2,
    /// Output amount below minimum (slippage exceeded)
    SlippageExceeded = 3,
    /// Arithmetic overflow
    Overflow = 4,
    /// Fee parameters are invalid
    InvalidFee = 5,
    /// Input amount is zero
    ZeroAmount = 6,
    /// Pool account addresses do not match recorded addresses
    InvalidPoolAccounts = 7,
}

impl From<DexError> for ProgramError {
    fn from(e: DexError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
