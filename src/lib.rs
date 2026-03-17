use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo,
    entrypoint,
    entrypoint::ProgramResult,
    pubkey::Pubkey,
};

pub mod error;
pub mod math;
pub mod processor;
pub mod state;

use processor::Processor;

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = DexInstruction::try_from_slice(instruction_data)
        .map_err(|_| solana_program::program_error::ProgramError::InvalidInstructionData)?;
    Processor::process(program_id, accounts, instruction)
}

/// Instructions supported by the DEX program
#[derive(BorshSerialize, BorshDeserialize)]
pub enum DexInstruction {
    /// Create and configure a new liquidity pool.
    ///
    /// Accounts (in order):
    /// 0. `[signer, writable]` pool_account      – pre-allocated, size = POOL_SIZE
    /// 1. `[]`                  token_a_mint
    /// 2. `[]`                  token_b_mint
    /// 3. `[writable]`          token_a_vault     – SPL token account, authority = pool_authority PDA
    /// 4. `[writable]`          token_b_vault     – SPL token account, authority = pool_authority PDA
    /// 5. `[writable]`          lp_mint           – mint authority = pool_authority PDA
    /// 6. `[]`                  pool_authority    – PDA: seeds = ["pool_authority", pool_account]
    /// 7. `[]`                  token_program
    InitializePool {
        fee_numerator: u64,    // e.g., 3  → 0.3% fee
        fee_denominator: u64,  // e.g., 1000
        bump: u8,              // canonical bump for pool_authority PDA
    },

    /// Deposit tokens into the pool and receive LP tokens.
    ///
    /// Accounts (in order):
    /// 0. `[signer]`   user
    /// 1. `[writable]` pool_account
    /// 2. `[writable]` user_token_a      – source of token A
    /// 3. `[writable]` token_a_vault
    /// 4. `[writable]` user_token_b      – source of token B
    /// 5. `[writable]` token_b_vault
    /// 6. `[writable]` lp_mint
    /// 7. `[writable]` user_lp_account   – destination for LP tokens
    /// 8. `[]`         pool_authority PDA
    /// 9. `[]`         token_program
    AddLiquidity {
        token_a_amount: u64,
        token_b_amount: u64,
        min_lp_amount: u64,   // slippage guard: revert if LP minted < this
    },

    /// Burn LP tokens and withdraw the proportional share of both tokens.
    ///
    /// Accounts (in order):
    /// 0. `[signer]`   user
    /// 1. `[writable]` pool_account
    /// 2. `[writable]` user_lp_account   – LP tokens burned from here
    /// 3. `[writable]` lp_mint
    /// 4. `[writable]` token_a_vault
    /// 5. `[writable]` user_token_a      – receives token A
    /// 6. `[writable]` token_b_vault
    /// 7. `[writable]` user_token_b      – receives token B
    /// 8. `[]`         pool_authority PDA
    /// 9. `[]`         token_program
    RemoveLiquidity {
        lp_amount: u64,
        min_token_a: u64,   // slippage guard
        min_token_b: u64,   // slippage guard
    },

    /// Swap tokens using the constant-product formula (x × y = k).
    ///
    /// Accounts (in order):
    /// 0. `[signer]`   user
    /// 1. `[writable]` pool_account
    /// 2. `[writable]` user_source_token  – tokens going IN
    /// 3. `[writable]` pool_source_vault  – pool vault receiving input
    /// 4. `[writable]` pool_dest_vault    – pool vault sending output
    /// 5. `[writable]` user_dest_token    – tokens going OUT
    /// 6. `[]`         pool_authority PDA
    /// 7. `[]`         token_program
    Swap {
        amount_in: u64,
        min_amount_out: u64, // slippage guard
        a_to_b: bool,        // true → swap A→B, false → swap B→A
    },
}
