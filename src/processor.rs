use crate::{
    DexInstruction,
    error::DexError,
    math,
    state::{Pool, POOL_SIZE},
};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{rent::Rent, Sysvar},
};

pub struct Processor;

impl Processor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction: DexInstruction,
    ) -> ProgramResult {
        match instruction {
            DexInstruction::InitializePool { fee_numerator, fee_denominator, bump } => {
                msg!("DEX: Initialize Pool");
                Self::process_initialize_pool(program_id, accounts, fee_numerator, fee_denominator, bump)
            }
            DexInstruction::AddLiquidity { token_a_amount, token_b_amount, min_lp_amount } => {
                msg!("DEX: Add Liquidity");
                Self::process_add_liquidity(program_id, accounts, token_a_amount, token_b_amount, min_lp_amount)
            }
            DexInstruction::RemoveLiquidity { lp_amount, min_token_a, min_token_b } => {
                msg!("DEX: Remove Liquidity");
                Self::process_remove_liquidity(program_id, accounts, lp_amount, min_token_a, min_token_b)
            }
            DexInstruction::Swap { amount_in, min_amount_out, a_to_b } => {
                msg!("DEX: Swap");
                Self::process_swap(program_id, accounts, amount_in, min_amount_out, a_to_b)
            }
        }
    }

    // ─────────────────────────── InitializePool ───────────────────────────
    //
    // Required accounts (in order):
    // 0. [signer, writable] pool_account – pre-allocated with POOL_SIZE bytes
    // 1. []                 token_a_mint
    // 2. []                 token_b_mint
    // 3. [writable]         token_a_vault – spl-token account, authority = pool_authority
    // 4. [writable]         token_b_vault – spl-token account, authority = pool_authority
    // 5. [writable]         lp_mint       – mint authority = pool_authority
    // 6. []                 pool_authority PDA
    // 7. []                 token_program
    fn process_initialize_pool(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        fee_numerator: u64,
        fee_denominator: u64,
        bump: u8,
    ) -> ProgramResult {
        let iter = &mut accounts.iter();
        let pool_account    = next_account_info(iter)?;
        let token_a_mint    = next_account_info(iter)?;
        let token_b_mint    = next_account_info(iter)?;
        let token_a_vault   = next_account_info(iter)?;
        let token_b_vault   = next_account_info(iter)?;
        let lp_mint         = next_account_info(iter)?;
        let pool_authority  = next_account_info(iter)?;
        let _token_program  = next_account_info(iter)?;

        // Validate fee parameters
        if fee_denominator == 0 || fee_numerator >= fee_denominator {
            return Err(DexError::InvalidFee.into());
        }

        // Verify pool_authority PDA
        let authority_seeds = &[b"pool_authority".as_ref(), pool_account.key.as_ref()];
        let (expected_authority, expected_bump) =
            Pubkey::find_program_address(authority_seeds, program_id);
        if pool_authority.key != &expected_authority || bump != expected_bump {
            return Err(ProgramError::InvalidAccountData);
        }

        // Ensure enough space
        if pool_account.data_len() < POOL_SIZE {
            return Err(ProgramError::AccountDataTooSmall);
        }

        // Guard against double-init
        if let Ok(p) = Pool::try_from_slice(&pool_account.data.borrow()) {
            if p.is_initialized {
                return Err(DexError::AlreadyInitialized.into());
            }
        }

        // Persist pool state
        let pool = Pool {
            is_initialized:  true,
            token_a_mint:    *token_a_mint.key,
            token_b_mint:    *token_b_mint.key,
            token_a_vault:   *token_a_vault.key,
            token_b_vault:   *token_b_vault.key,
            lp_mint:         *lp_mint.key,
            reserve_a:       0,
            reserve_b:       0,
            lp_supply:       0,
            fee_numerator,
            fee_denominator,
            bump,
        };
        pool.serialize(&mut *pool_account.data.borrow_mut())?;

        msg!("Pool initialized: {}", pool_account.key);
        msg!("  Token A : {}", token_a_mint.key);
        msg!("  Token B : {}", token_b_mint.key);
        msg!("  Fee     : {}/{}", fee_numerator, fee_denominator);
        Ok(())
    }

    // ─────────────────────────── AddLiquidity ─────────────────────────────
    //
    // Required accounts:
    // 0. [signer]   user
    // 1. [writable] pool_account
    // 2. [writable] user_token_a  (transfers to vault)
    // 3. [writable] token_a_vault
    // 4. [writable] user_token_b  (transfers to vault)
    // 5. [writable] token_b_vault
    // 6. [writable] lp_mint
    // 7. [writable] user_lp_account (receives minted LP tokens)
    // 8. []         pool_authority PDA
    // 9. []         token_program
    fn process_add_liquidity(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        token_a_amount: u64,
        token_b_amount: u64,
        min_lp_amount: u64,
    ) -> ProgramResult {
        if token_a_amount == 0 || token_b_amount == 0 {
            return Err(DexError::ZeroAmount.into());
        }

        let iter = &mut accounts.iter();
        let user            = next_account_info(iter)?;
        let pool_account    = next_account_info(iter)?;
        let user_token_a    = next_account_info(iter)?;
        let token_a_vault   = next_account_info(iter)?;
        let user_token_b    = next_account_info(iter)?;
        let token_b_vault   = next_account_info(iter)?;
        let lp_mint         = next_account_info(iter)?;
        let user_lp_account = next_account_info(iter)?;
        let pool_authority  = next_account_info(iter)?;
        let token_program   = next_account_info(iter)?;

        let mut pool = Pool::try_from_slice(&pool_account.data.borrow())?;
        if !pool.is_initialized {
            return Err(DexError::NotInitialized.into());
        }

        // Calculate LP tokens to mint
        let lp_amount = math::calculate_lp_tokens(
            token_a_amount, token_b_amount,
            pool.reserve_a, pool.reserve_b, pool.lp_supply,
        ).ok_or(DexError::Overflow)?;

        if lp_amount == 0 {
            return Err(DexError::ZeroAmount.into());
        }
        if lp_amount < min_lp_amount {
            return Err(DexError::SlippageExceeded.into());
        }

        let bump = pool.bump;
        let authority_seeds: &[&[u8]] = &[b"pool_authority", pool_account.key.as_ref(), &[bump]];

        // Transfer A: user → vault
        invoke(
            &spl_token::instruction::transfer(
                token_program.key, user_token_a.key, token_a_vault.key,
                user.key, &[], token_a_amount,
            )?,
            &[user_token_a.clone(), token_a_vault.clone(), user.clone(), token_program.clone()],
        )?;

        // Transfer B: user → vault
        invoke(
            &spl_token::instruction::transfer(
                token_program.key, user_token_b.key, token_b_vault.key,
                user.key, &[], token_b_amount,
            )?,
            &[user_token_b.clone(), token_b_vault.clone(), user.clone(), token_program.clone()],
        )?;

        // Mint LP tokens → user
        invoke_signed(
            &spl_token::instruction::mint_to(
                token_program.key, lp_mint.key, user_lp_account.key,
                pool_authority.key, &[], lp_amount,
            )?,
            &[lp_mint.clone(), user_lp_account.clone(), pool_authority.clone(), token_program.clone()],
            &[authority_seeds],
        )?;

        // Update state
        pool.reserve_a = pool.reserve_a.checked_add(token_a_amount).ok_or(DexError::Overflow)?;
        pool.reserve_b = pool.reserve_b.checked_add(token_b_amount).ok_or(DexError::Overflow)?;
        pool.lp_supply = pool.lp_supply.checked_add(lp_amount).ok_or(DexError::Overflow)?;
        pool.serialize(&mut *pool_account.data.borrow_mut())?;

        msg!("Liquidity added: A={}, B={}, LP minted={}", token_a_amount, token_b_amount, lp_amount);
        Ok(())
    }

    // ─────────────────────────── RemoveLiquidity ──────────────────────────
    //
    // Required accounts:
    // 0. [signer]   user
    // 1. [writable] pool_account
    // 2. [writable] user_lp_account  (LP tokens burned from here)
    // 3. [writable] lp_mint
    // 4. [writable] token_a_vault
    // 5. [writable] user_token_a     (receives token A)
    // 6. [writable] token_b_vault
    // 7. [writable] user_token_b     (receives token B)
    // 8. []         pool_authority PDA
    // 9. []         token_program
    fn process_remove_liquidity(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        lp_amount: u64,
        min_token_a: u64,
        min_token_b: u64,
    ) -> ProgramResult {
        if lp_amount == 0 {
            return Err(DexError::ZeroAmount.into());
        }

        let iter = &mut accounts.iter();
        let user            = next_account_info(iter)?;
        let pool_account    = next_account_info(iter)?;
        let user_lp_account = next_account_info(iter)?;
        let lp_mint         = next_account_info(iter)?;
        let token_a_vault   = next_account_info(iter)?;
        let user_token_a    = next_account_info(iter)?;
        let token_b_vault   = next_account_info(iter)?;
        let user_token_b    = next_account_info(iter)?;
        let pool_authority  = next_account_info(iter)?;
        let token_program   = next_account_info(iter)?;

        let mut pool = Pool::try_from_slice(&pool_account.data.borrow())?;
        if !pool.is_initialized {
            return Err(DexError::NotInitialized.into());
        }

        let (amount_a, amount_b) = math::calculate_removal_amounts(
            lp_amount, pool.reserve_a, pool.reserve_b, pool.lp_supply,
        ).ok_or(DexError::InsufficientLiquidity)?;

        if amount_a < min_token_a || amount_b < min_token_b {
            return Err(DexError::SlippageExceeded.into());
        }

        let bump = pool.bump;
        let authority_seeds: &[&[u8]] = &[b"pool_authority", pool_account.key.as_ref(), &[bump]];

        // Burn LP tokens
        invoke(
            &spl_token::instruction::burn(
                token_program.key, user_lp_account.key, lp_mint.key,
                user.key, &[], lp_amount,
            )?,
            &[user_lp_account.clone(), lp_mint.clone(), user.clone(), token_program.clone()],
        )?;

        // Release token A: vault → user
        invoke_signed(
            &spl_token::instruction::transfer(
                token_program.key, token_a_vault.key, user_token_a.key,
                pool_authority.key, &[], amount_a,
            )?,
            &[token_a_vault.clone(), user_token_a.clone(), pool_authority.clone(), token_program.clone()],
            &[authority_seeds],
        )?;

        // Release token B: vault → user
        invoke_signed(
            &spl_token::instruction::transfer(
                token_program.key, token_b_vault.key, user_token_b.key,
                pool_authority.key, &[], amount_b,
            )?,
            &[token_b_vault.clone(), user_token_b.clone(), pool_authority.clone(), token_program.clone()],
            &[authority_seeds],
        )?;

        // Update state
        pool.reserve_a = pool.reserve_a.checked_sub(amount_a).ok_or(DexError::Overflow)?;
        pool.reserve_b = pool.reserve_b.checked_sub(amount_b).ok_or(DexError::Overflow)?;
        pool.lp_supply = pool.lp_supply.checked_sub(lp_amount).ok_or(DexError::Overflow)?;
        pool.serialize(&mut *pool_account.data.borrow_mut())?;

        msg!("Liquidity removed: LP={}, A={}, B={}", lp_amount, amount_a, amount_b);
        Ok(())
    }

    // ─────────────────────────── Swap ─────────────────────────────────────
    //
    // Required accounts:
    // 0. [signer]   user
    // 1. [writable] pool_account
    // 2. [writable] user_source_token   (tokens going IN)
    // 3. [writable] pool_source_vault   (pool vault receiving input tokens)
    // 4. [writable] pool_dest_vault     (pool vault sending output tokens)
    // 5. [writable] user_dest_token     (tokens going OUT)
    // 6. []         pool_authority PDA
    // 7. []         token_program
    fn process_swap(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount_in: u64,
        min_amount_out: u64,
        a_to_b: bool,
    ) -> ProgramResult {
        if amount_in == 0 {
            return Err(DexError::ZeroAmount.into());
        }

        let iter = &mut accounts.iter();
        let user              = next_account_info(iter)?;
        let pool_account      = next_account_info(iter)?;
        let user_source       = next_account_info(iter)?;
        let pool_source_vault = next_account_info(iter)?;
        let pool_dest_vault   = next_account_info(iter)?;
        let user_dest         = next_account_info(iter)?;
        let pool_authority    = next_account_info(iter)?;
        let token_program     = next_account_info(iter)?;

        let mut pool = Pool::try_from_slice(&pool_account.data.borrow())?;
        if !pool.is_initialized {
            return Err(DexError::NotInitialized.into());
        }
        if pool.reserve_a == 0 || pool.reserve_b == 0 {
            return Err(DexError::InsufficientLiquidity.into());
        }

        // Identify reserves for the chosen swap direction
        let (reserve_in, reserve_out) = if a_to_b {
            (pool.reserve_a, pool.reserve_b)
        } else {
            (pool.reserve_b, pool.reserve_a)
        };

        // Compute output using constant-product formula
        let amount_out = math::swap_output(
            amount_in, reserve_in, reserve_out,
            pool.fee_numerator, pool.fee_denominator,
        ).ok_or(DexError::Overflow)?;

        if amount_out == 0 {
            return Err(DexError::ZeroAmount.into());
        }
        if amount_out < min_amount_out {
            return Err(DexError::SlippageExceeded.into());
        }

        let bump = pool.bump;
        let authority_seeds: &[&[u8]] = &[b"pool_authority", pool_account.key.as_ref(), &[bump]];

        // Transfer input tokens from user → pool source vault
        invoke(
            &spl_token::instruction::transfer(
                token_program.key, user_source.key, pool_source_vault.key,
                user.key, &[], amount_in,
            )?,
            &[user_source.clone(), pool_source_vault.clone(), user.clone(), token_program.clone()],
        )?;

        // Transfer output tokens from pool dest vault → user
        invoke_signed(
            &spl_token::instruction::transfer(
                token_program.key, pool_dest_vault.key, user_dest.key,
                pool_authority.key, &[], amount_out,
            )?,
            &[pool_dest_vault.clone(), user_dest.clone(), pool_authority.clone(), token_program.clone()],
            &[authority_seeds],
        )?;

        // Update cached reserves
        if a_to_b {
            pool.reserve_a = pool.reserve_a.checked_add(amount_in).ok_or(DexError::Overflow)?;
            pool.reserve_b = pool.reserve_b.checked_sub(amount_out).ok_or(DexError::Overflow)?;
        } else {
            pool.reserve_b = pool.reserve_b.checked_add(amount_in).ok_or(DexError::Overflow)?;
            pool.reserve_a = pool.reserve_a.checked_sub(amount_out).ok_or(DexError::Overflow)?;
        }
        pool.serialize(&mut *pool_account.data.borrow_mut())?;

        msg!("Swap: in={}, out={}, a_to_b={}", amount_in, amount_out, a_to_b);
        Ok(())
    }
}
