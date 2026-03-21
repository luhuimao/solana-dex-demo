/// AMM math: Constant Product Market Maker (x * y = k)
/// Ported from the native solana_dex_demo — logic is identical.

/// Calculate the output amount for a swap using the constant-product formula.
///
/// amount_out = (reserve_out × amount_in × fee_factor) /
///              (reserve_in × fee_denominator + amount_in × fee_factor)
///
/// fee_factor = fee_denominator − fee_numerator
pub fn swap_output(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    fee_numerator: u64,
    fee_denominator: u64,
) -> Option<u64> {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 || fee_denominator == 0 {
        return None;
    }
    let fee_factor = fee_denominator.checked_sub(fee_numerator)?;

    let numerator = (reserve_out as u128)
        .checked_mul(amount_in as u128)?
        .checked_mul(fee_factor as u128)?;

    let denominator = (reserve_in as u128)
        .checked_mul(fee_denominator as u128)?
        .checked_add((amount_in as u128).checked_mul(fee_factor as u128)?)?;

    if denominator == 0 {
        return None;
    }
    u64::try_from(numerator.checked_div(denominator)?).ok()
}

/// Calculate LP tokens to mint when adding liquidity.
///
/// First deposit:  LP = √(amount_a × amount_b)
/// Subsequent:     LP = min(amount_a / reserve_a, amount_b / reserve_b) × lp_supply
pub fn calculate_lp_tokens(
    token_a_amount: u64,
    token_b_amount: u64,
    reserve_a: u64,
    reserve_b: u64,
    total_supply: u64,
) -> Option<u64> {
    if total_supply == 0 {
        let product = (token_a_amount as u128).checked_mul(token_b_amount as u128)?;
        u64::try_from(integer_sqrt(product)).ok()
    } else {
        if reserve_a == 0 || reserve_b == 0 {
            return None;
        }
        let lp_a = (token_a_amount as u128)
            .checked_mul(total_supply as u128)?
            .checked_div(reserve_a as u128)?;
        let lp_b = (token_b_amount as u128)
            .checked_mul(total_supply as u128)?
            .checked_div(reserve_b as u128)?;
        u64::try_from(lp_a.min(lp_b)).ok()
    }
}

/// Calculate tokens to return when removing liquidity.
///
/// amount_a = lp_amount × reserve_a / total_supply
/// amount_b = lp_amount × reserve_b / total_supply
pub fn calculate_removal_amounts(
    lp_amount: u64,
    reserve_a: u64,
    reserve_b: u64,
    total_supply: u64,
) -> Option<(u64, u64)> {
    if total_supply == 0 {
        return None;
    }
    let a = (lp_amount as u128)
        .checked_mul(reserve_a as u128)?
        .checked_div(total_supply as u128)?;
    let b = (lp_amount as u128)
        .checked_mul(reserve_b as u128)?
        .checked_div(total_supply as u128)?;
    Some((u64::try_from(a).ok()?, u64::try_from(b).ok()?))
}

/// Integer square root (Babylonian method)
fn integer_sqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}
