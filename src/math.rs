/// AMM math: Constant Product Market Maker (x * y = k)

/// Calculate the output amount for a swap using the constant product formula.
///
/// Formula (with fee):
///   amount_out = (reserve_out × amount_in × fee_factor) /
///                (reserve_in × fee_denominator + amount_in × fee_factor)
///
/// where fee_factor = fee_denominator - fee_numerator
///
/// Example: 0.3% fee → fee_numerator=3, fee_denominator=1000, fee_factor=997
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

    // Use u128 to avoid overflow during intermediate calculations
    let numerator = (reserve_out as u128)
        .checked_mul(amount_in as u128)?
        .checked_mul(fee_factor as u128)?;

    let denominator = (reserve_in as u128)
        .checked_mul(fee_denominator as u128)?
        .checked_add((amount_in as u128).checked_mul(fee_factor as u128)?)?;

    if denominator == 0 {
        return None;
    }

    let amount_out = numerator.checked_div(denominator)?;
    u64::try_from(amount_out).ok()
}

/// Calculate the LP tokens to mint when adding liquidity.
///
/// - First deposit: LP = √(amount_a × amount_b)  (geometric mean)
/// - Subsequent:    LP = min(amount_a/reserve_a, amount_b/reserve_b) × total_supply
pub fn calculate_lp_tokens(
    token_a_amount: u64,
    token_b_amount: u64,
    reserve_a: u64,
    reserve_b: u64,
    total_supply: u64,
) -> Option<u64> {
    if total_supply == 0 {
        // First liquidity: set initial price as square root of product
        let product = (token_a_amount as u128).checked_mul(token_b_amount as u128)?;
        let lp = integer_sqrt(product);
        u64::try_from(lp).ok()
    } else {
        if reserve_a == 0 || reserve_b == 0 {
            return None;
        }
        // Proportional to the smaller ratio (penalises unbalanced deposits)
        let lp_a = (token_a_amount as u128)
            .checked_mul(total_supply as u128)?
            .checked_div(reserve_a as u128)?;
        let lp_b = (token_b_amount as u128)
            .checked_mul(total_supply as u128)?
            .checked_div(reserve_b as u128)?;

        let lp = lp_a.min(lp_b);
        u64::try_from(lp).ok()
    }
}

/// Calculate how many tokens A and B to return when removing liquidity.
///
///   amount_a = lp_amount × reserve_a / total_supply
///   amount_b = lp_amount × reserve_b / total_supply
pub fn calculate_removal_amounts(
    lp_amount: u64,
    reserve_a: u64,
    reserve_b: u64,
    total_supply: u64,
) -> Option<(u64, u64)> {
    if total_supply == 0 {
        return None;
    }

    let amount_a = (lp_amount as u128)
        .checked_mul(reserve_a as u128)?
        .checked_div(total_supply as u128)?;
    let amount_b = (lp_amount as u128)
        .checked_mul(reserve_b as u128)?
        .checked_div(total_supply as u128)?;

    Some((u64::try_from(amount_a).ok()?, u64::try_from(amount_b).ok()?))
}

/// Integer square root (Babylonian / Newton's method)
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

// ─────────────────────────── unit tests ───────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_output_basic() {
        // 100 A in, reserves 1000 A / 1000 B, 0.3% fee
        let out = swap_output(100, 1000, 1000, 3, 1000).unwrap();
        // Theoretical: 1000*100*997 / (1000*1000 + 100*997) ≈ 90.66
        assert!(out >= 90 && out <= 91, "Expected ~90, got {}", out);
    }

    #[test]
    fn test_swap_output_asymmetric_reserves() {
        // Small amount in, deep reserve on out side
        let out = swap_output(10, 100, 10_000, 3, 1000).unwrap();
        assert!(out > 0, "Should produce non-zero output");
    }

    #[test]
    fn test_swap_output_zero_returns_none() {
        assert!(swap_output(0, 1000, 1000, 3, 1000).is_none());
        assert!(swap_output(100, 0, 1000, 3, 1000).is_none());
    }

    #[test]
    fn test_lp_first_deposit_equal() {
        // sqrt(1000 * 1000) = 1000
        let lp = calculate_lp_tokens(1000, 1000, 0, 0, 0).unwrap();
        assert_eq!(lp, 1000);
    }

    #[test]
    fn test_lp_first_deposit_unequal() {
        // sqrt(400 * 900) = sqrt(360000) = 600
        let lp = calculate_lp_tokens(400, 900, 0, 0, 0).unwrap();
        assert_eq!(lp, 600);
    }

    #[test]
    fn test_lp_subsequent_deposit() {
        // Existing: 1000 A, 1000 B, 1000 LP
        // Adding  :  100 A,  100 B → LP = 100
        let lp = calculate_lp_tokens(100, 100, 1000, 1000, 1000).unwrap();
        assert_eq!(lp, 100);
    }

    #[test]
    fn test_lp_subsequent_deposit_gives_min() {
        // Adding more B than A relative to ratio → limited by A side
        let lp = calculate_lp_tokens(50, 200, 1000, 1000, 1000).unwrap();
        assert_eq!(lp, 50, "Should be limited by the smaller ratio (A side)");
    }

    #[test]
    fn test_removal_amounts_proportional() {
        // Remove 100 LP from 1000 total: get 10% of reserves
        let (a, b) = calculate_removal_amounts(100, 500, 800, 1000).unwrap();
        assert_eq!(a, 50);
        assert_eq!(b, 80);
    }

    #[test]
    fn test_removal_amounts_zero_supply_returns_none() {
        assert!(calculate_removal_amounts(100, 500, 800, 0).is_none());
    }

    #[test]
    fn test_constant_product_invariant() {
        // After swap, k = reserve_in_new × reserve_out_new should be ≥ k_old
        let reserve_in = 1000u64;
        let reserve_out = 1000u64;
        let amount_in = 100u64;
        let out = swap_output(amount_in, reserve_in, reserve_out, 3, 1000).unwrap();
        let new_reserve_in = reserve_in + amount_in;
        let new_reserve_out = reserve_out - out;
        // k should be >= original k (fee causes slight increase)
        assert!(
            (new_reserve_in as u128) * (new_reserve_out as u128)
                >= (reserve_in as u128) * (reserve_out as u128)
        );
    }
}
