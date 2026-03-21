use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Burn, Mint, MintTo, TokenAccount, TokenInterface, Transfer, TransferChecked};

pub mod error;
pub mod math;
pub mod state;

use error::DexError;
use state::{Pool, POOL_SPACE};

declare_id!("GbaRn6v3mVcHQYsh5ZEP81iP7Qg1CpEgzYi6bP2gd6AT");

#[program]
pub mod anchor_dex {
    use super::*;

    // ─────────────────────────── InitializePool ───────────────────────────
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        fee_numerator: u64,
        fee_denominator: u64,
    ) -> Result<()> {
        require!(
            fee_denominator > 0 && fee_numerator < fee_denominator,
            DexError::InvalidFee
        );

        let pool = &mut ctx.accounts.pool;
        let bump = ctx.bumps.pool_authority;
        let pool_key = pool.key();

        pool.token_a_mint    = ctx.accounts.token_a_mint.key();
        pool.token_b_mint    = ctx.accounts.token_b_mint.key();
        pool.token_a_vault   = ctx.accounts.token_a_vault.key();
        pool.token_b_vault   = ctx.accounts.token_b_vault.key();
        pool.lp_mint         = ctx.accounts.lp_mint.key();
        pool.reserve_a       = 0;
        pool.reserve_b       = 0;
        pool.lp_supply       = 0;
        pool.fee_numerator   = fee_numerator;
        pool.fee_denominator = fee_denominator;
        pool.bump            = bump;

        msg!("Pool initialized: {}", pool_key);
        msg!("  Token A : {}", pool.token_a_mint);
        msg!("  Token B : {}", pool.token_b_mint);
        msg!("  Fee     : {}/{}", fee_numerator, fee_denominator);

        Ok(())
    }

    // ─────────────────────────── AddLiquidity ─────────────────────────────
    pub fn add_liquidity(
        ctx: Context<AddLiquidity>,
        token_a_amount: u64,
        token_b_amount: u64,
        min_lp_amount: u64,
    ) -> Result<()> {
        require!(token_a_amount > 0 && token_b_amount > 0, DexError::ZeroAmount);

        let pool = &ctx.accounts.pool;

        let lp_amount = math::calculate_lp_tokens(
            token_a_amount,
            token_b_amount,
            pool.reserve_a,
            pool.reserve_b,
            pool.lp_supply,
        )
        .ok_or(DexError::Overflow)?;

        require!(lp_amount > 0, DexError::ZeroAmount);
        require!(lp_amount >= min_lp_amount, DexError::SlippageExceeded);

        // Transfer token A: user → vault
        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from:      ctx.accounts.user_token_a.to_account_info(),
                    mint:      ctx.accounts.token_a_mint.to_account_info(),
                    to:        ctx.accounts.token_a_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            token_a_amount,
            ctx.accounts.token_a_mint.decimals,
        )?;

        // Transfer token B: user → vault
        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from:      ctx.accounts.user_token_b.to_account_info(),
                    mint:      ctx.accounts.token_b_mint.to_account_info(),
                    to:        ctx.accounts.token_b_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            token_b_amount,
            ctx.accounts.token_b_mint.decimals,
        )?;

        // Mint LP tokens → user (signed by pool_authority PDA)
        let pool_key = ctx.accounts.pool.key();
        let authority_seeds: &[&[u8]] =
            &[b"pool_authority", pool_key.as_ref(), &[pool.bump]];

        token_interface::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint:      ctx.accounts.lp_mint.to_account_info(),
                    to:        ctx.accounts.user_lp.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info(),
                },
                &[authority_seeds],
            ),
            lp_amount,
        )?;

        // Update reserves
        let pool = &mut ctx.accounts.pool;
        pool.reserve_a = pool.reserve_a.checked_add(token_a_amount).ok_or(DexError::Overflow)?;
        pool.reserve_b = pool.reserve_b.checked_add(token_b_amount).ok_or(DexError::Overflow)?;
        pool.lp_supply = pool.lp_supply.checked_add(lp_amount).ok_or(DexError::Overflow)?;

        msg!("Liquidity added: A={}, B={}, LP minted={}", token_a_amount, token_b_amount, lp_amount);
        Ok(())
    }

    // ─────────────────────────── RemoveLiquidity ──────────────────────────
    pub fn remove_liquidity(
        ctx: Context<RemoveLiquidity>,
        lp_amount: u64,
        min_token_a: u64,
        min_token_b: u64,
    ) -> Result<()> {
        require!(lp_amount > 0, DexError::ZeroAmount);

        let pool = &ctx.accounts.pool;

        let (amount_a, amount_b) = math::calculate_removal_amounts(
            lp_amount,
            pool.reserve_a,
            pool.reserve_b,
            pool.lp_supply,
        )
        .ok_or(DexError::InsufficientLiquidity)?;

        require!(amount_a >= min_token_a && amount_b >= min_token_b, DexError::SlippageExceeded);

        // Burn LP tokens (user is authority)
        token_interface::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint:      ctx.accounts.lp_mint.to_account_info(),
                    from:      ctx.accounts.user_lp.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            lp_amount,
        )?;

        let pool_key = ctx.accounts.pool.key();
        let authority_seeds: &[&[u8]] =
            &[b"pool_authority", pool_key.as_ref(), &[pool.bump]];

        // Release token A: vault → user
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from:      ctx.accounts.token_a_vault.to_account_info(),
                    mint:      ctx.accounts.token_a_mint.to_account_info(),
                    to:        ctx.accounts.user_token_a.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info(),
                },
                &[authority_seeds],
            ),
            amount_a,
            ctx.accounts.token_a_mint.decimals,
        )?;

        // Release token B: vault → user
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from:      ctx.accounts.token_b_vault.to_account_info(),
                    mint:      ctx.accounts.token_b_mint.to_account_info(),
                    to:        ctx.accounts.user_token_b.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info(),
                },
                &[authority_seeds],
            ),
            amount_b,
            ctx.accounts.token_b_mint.decimals,
        )?;

        // Update reserves
        let pool = &mut ctx.accounts.pool;
        pool.reserve_a = pool.reserve_a.checked_sub(amount_a).ok_or(DexError::Overflow)?;
        pool.reserve_b = pool.reserve_b.checked_sub(amount_b).ok_or(DexError::Overflow)?;
        pool.lp_supply = pool.lp_supply.checked_sub(lp_amount).ok_or(DexError::Overflow)?;

        msg!("Liquidity removed: LP={}, A={}, B={}", lp_amount, amount_a, amount_b);
        Ok(())
    }

    // ─────────────────────────── Swap ─────────────────────────────────────
    pub fn swap(
        ctx: Context<Swap>,
        amount_in: u64,
        min_amount_out: u64,
        a_to_b: bool,
    ) -> Result<()> {
        require!(amount_in > 0, DexError::ZeroAmount);

        let pool = &ctx.accounts.pool;
        require!(pool.reserve_a > 0 && pool.reserve_b > 0, DexError::InsufficientLiquidity);

        let (reserve_in, reserve_out) = if a_to_b {
            (pool.reserve_a, pool.reserve_b)
        } else {
            (pool.reserve_b, pool.reserve_a)
        };

        let amount_out = math::swap_output(
            amount_in, reserve_in, reserve_out,
            pool.fee_numerator, pool.fee_denominator,
        )
        .ok_or(DexError::Overflow)?;

        require!(amount_out > 0, DexError::ZeroAmount);
        require!(amount_out >= min_amount_out, DexError::SlippageExceeded);

        // Determine mint decimals for transfer_checked
        let (source_mint, source_decimals, dest_mint, dest_decimals) = if a_to_b {
            (
                ctx.accounts.token_a_mint.to_account_info(),
                ctx.accounts.token_a_mint.decimals,
                ctx.accounts.token_b_mint.to_account_info(),
                ctx.accounts.token_b_mint.decimals,
            )
        } else {
            (
                ctx.accounts.token_b_mint.to_account_info(),
                ctx.accounts.token_b_mint.decimals,
                ctx.accounts.token_a_mint.to_account_info(),
                ctx.accounts.token_a_mint.decimals,
            )
        };

        // User → pool source vault
        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from:      ctx.accounts.user_source.to_account_info(),
                    mint:      source_mint,
                    to:        ctx.accounts.pool_source_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount_in,
            source_decimals,
        )?;

        let pool_key = ctx.accounts.pool.key();
        let authority_seeds: &[&[u8]] =
            &[b"pool_authority", pool_key.as_ref(), &[pool.bump]];

        // Pool dest vault → user
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from:      ctx.accounts.pool_dest_vault.to_account_info(),
                    mint:      dest_mint,
                    to:        ctx.accounts.user_dest.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info(),
                },
                &[authority_seeds],
            ),
            amount_out,
            dest_decimals,
        )?;

        // Update reserves
        let pool = &mut ctx.accounts.pool;
        if a_to_b {
            pool.reserve_a = pool.reserve_a.checked_add(amount_in).ok_or(DexError::Overflow)?;
            pool.reserve_b = pool.reserve_b.checked_sub(amount_out).ok_or(DexError::Overflow)?;
        } else {
            pool.reserve_b = pool.reserve_b.checked_add(amount_in).ok_or(DexError::Overflow)?;
            pool.reserve_a = pool.reserve_a.checked_sub(amount_out).ok_or(DexError::Overflow)?;
        }

        msg!("Swap: in={}, out={}, a_to_b={}", amount_in, amount_out, a_to_b);
        Ok(())
    }

    // ─────────────────────────── GetPoolInfo ──────────────────────────────
    pub fn get_pool_info(ctx: Context<GetPoolInfo>) -> Result<()> {
        let pool = &ctx.accounts.pool;

        msg!("=== Pool Info ===");
        msg!("Pool account : {}", ctx.accounts.pool.key());
        msg!("Token A mint : {}", pool.token_a_mint);
        msg!("Token B mint : {}", pool.token_b_mint);
        msg!("Token A vault: {}", pool.token_a_vault);
        msg!("Token B vault: {}", pool.token_b_vault);
        msg!("LP mint      : {}", pool.lp_mint);
        msg!("Reserve A    : {}", pool.reserve_a);
        msg!("Reserve B    : {}", pool.reserve_b);
        msg!("LP supply    : {}", pool.lp_supply);
        msg!("Fee          : {}/{}", pool.fee_numerator, pool.fee_denominator);

        if pool.reserve_a > 0 {
            let p = pool.reserve_b.saturating_mul(1_000_000) / pool.reserve_a;
            msg!("Price A→B    : {}.{:06} B/A (×1e6={})", p / 1_000_000, p % 1_000_000, p);
        } else {
            msg!("Price A→B    : n/a (empty pool)");
        }

        match pool.reserve_a.checked_mul(pool.reserve_b) {
            Some(k) => msg!("k (A×B)      : {}", k),
            None    => msg!("k (A×B)      : overflow"),
        }

        if pool.lp_supply > 0 {
            let a_per = pool.reserve_a.saturating_mul(1_000_000) / pool.lp_supply;
            let b_per = pool.reserve_b.saturating_mul(1_000_000) / pool.lp_supply;
            msg!("A per LP×1e6 : {}", a_per);
            msg!("B per LP×1e6 : {}", b_per);
        }

        msg!("=== End Pool Info ===");
        Ok(())
    }
}

// ─────────────────────────── Account contexts ─────────────────────────────

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = user,
        space = POOL_SPACE,
    )]
    pub pool: Account<'info, Pool>,

    pub token_a_mint: InterfaceAccount<'info, Mint>,
    pub token_b_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = user,
        token::mint     = token_a_mint,
        token::authority = pool_authority,
        token::token_program = token_program,
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = user,
        token::mint     = token_b_mint,
        token::authority = pool_authority,
        token::token_program = token_program,
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = user,
        mint::decimals   = 6,
        mint::authority  = pool_authority,
        mint::token_program = token_program,
    )]
    pub lp_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: PDA used only as authority; validated by seeds
    #[account(
        seeds = [b"pool_authority", pool.key().as_ref()],
        bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    #[account(mut)]
    pub pool: Box<Account<'info, Pool>>,

    pub token_a_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_b_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, token::mint = pool.token_a_mint, token::token_program = token_program)]
    pub user_token_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, address = pool.token_a_vault)]
    pub token_a_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = pool.token_b_mint, token::token_program = token_program)]
    pub user_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, address = pool.token_b_vault)]
    pub token_b_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, address = pool.lp_mint)]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, token::mint = pool.lp_mint, token::token_program = token_program)]
    pub user_lp: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: PDA; seeds validated
    #[account(
        seeds = [b"pool_authority", pool.key().as_ref()],
        bump = pool.bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

    pub user: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct RemoveLiquidity<'info> {
    #[account(mut)]
    pub pool: Box<Account<'info, Pool>>,

    pub token_a_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_b_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, token::mint = pool.lp_mint, token::token_program = token_program)]
    pub user_lp: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, address = pool.lp_mint)]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, address = pool.token_a_vault)]
    pub token_a_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = pool.token_a_mint, token::token_program = token_program)]
    pub user_token_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, address = pool.token_b_vault)]
    pub token_b_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = pool.token_b_mint, token::token_program = token_program)]
    pub user_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: PDA; seeds validated
    #[account(
        seeds = [b"pool_authority", pool.key().as_ref()],
        bump = pool.bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

    pub user: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub pool: Box<Account<'info, Pool>>,

    pub token_a_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_b_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Source token account owned by the user (tokens going IN)
    #[account(mut)]
    pub user_source: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Pool vault receiving the input tokens
    #[account(mut)]
    pub pool_source_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Pool vault sending the output tokens
    #[account(mut)]
    pub pool_dest_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Destination token account owned by the user (tokens going OUT)
    #[account(mut)]
    pub user_dest: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: PDA; seeds validated
    #[account(
        seeds = [b"pool_authority", pool.key().as_ref()],
        bump = pool.bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

    pub user: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct GetPoolInfo<'info> {
    pub pool: Account<'info, Pool>,
}
