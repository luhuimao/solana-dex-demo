use borsh::BorshDeserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction, system_program, sysvar,
    transaction::Transaction,
};
use solana_dex_demo::{state::{Pool, POOL_SIZE}, DexInstruction};
use spl_token::state::{Account as TokenAccount, Mint};
use std::str::FromStr;

// Helper function to create a mint
fn create_mint(
    client: &RpcClient,
    payer: &Keypair,
    mint_authority: &Pubkey,
    decimals: u8,
) -> Keypair {
    let mint = Keypair::new();
    let rent = client
        .get_minimum_balance_for_rent_exemption(Mint::LEN)
        .unwrap();

    let create_account_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent,
        Mint::LEN as u64,
        &spl_token::id(),
    );

    let init_mint_ix = spl_token::instruction::initialize_mint(
        &spl_token::id(),
        &mint.pubkey(),
        mint_authority,
        None,
        decimals,
    )
    .unwrap();

    let mut tx = Transaction::new_with_payer(
        &[create_account_ix, init_mint_ix],
        Some(&payer.pubkey()),
    );
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[payer, &mint], blockhash);
    client.send_and_confirm_transaction(&tx).unwrap();

    mint
}

// Helper function to create a token account
fn create_token_account(
    client: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Keypair {
    let account = Keypair::new();
    let rent = client
        .get_minimum_balance_for_rent_exemption(TokenAccount::LEN)
        .unwrap();

    let create_account_ix = system_instruction::create_account(
        &payer.pubkey(),
        &account.pubkey(),
        rent,
        TokenAccount::LEN as u64,
        &spl_token::id(),
    );

    let init_account_ix = spl_token::instruction::initialize_account(
        &spl_token::id(),
        &account.pubkey(),
        mint,
        owner,
    )
    .unwrap();

    let mut tx = Transaction::new_with_payer(
        &[create_account_ix, init_account_ix],
        Some(&payer.pubkey()),
    );
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[payer, &account], blockhash);
    client.send_and_confirm_transaction(&tx).unwrap();

    account
}

// Helper function to mint tokens
fn mint_to(
    client: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    destination: &Pubkey,
    authority: &Keypair,
    amount: u64,
) {
    let ix = spl_token::instruction::mint_to(
        &spl_token::id(),
        mint,
        destination,
        &authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();

    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[payer, authority], blockhash);
    client.send_and_confirm_transaction(&tx).unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: read pool account data locally and pretty-print the state.
// No transaction is required – we simply call get_account_data() and
// deserialise the Borsh bytes with Pool::try_from_slice().
// ─────────────────────────────────────────────────────────────────────────────
fn print_pool_info_local(client: &RpcClient, pool_pubkey: &Pubkey) {
    println!("\n  [Local] Fetching pool account: {}", pool_pubkey);
    match client.get_account_data(pool_pubkey) {
        Err(e) => {
            eprintln!("  [Local] Failed to fetch pool account: {:?}", e);
            return;
        }
        Ok(data) => {
            match Pool::try_from_slice(&data) {
                Err(e) => eprintln!("  [Local] Failed to deserialize pool: {:?}", e),
                Ok(pool) => {
                    println!("  ┌─ Pool State (local read) ─────────────────────");
                    println!("  │ initialized  : {}", pool.is_initialized);
                    println!("  │ token A mint : {}", pool.token_a_mint);
                    println!("  │ token B mint : {}", pool.token_b_mint);
                    println!("  │ vault A      : {}", pool.token_a_vault);
                    println!("  │ vault B      : {}", pool.token_b_vault);
                    println!("  │ LP mint      : {}", pool.lp_mint);
                    println!("  │ reserve A    : {}", pool.reserve_a);
                    println!("  │ reserve B    : {}", pool.reserve_b);
                    println!("  │ LP supply    : {}", pool.lp_supply);
                    println!("  │ fee          : {}/{}", pool.fee_numerator, pool.fee_denominator);
                    // Spot price (×1e6, integer arithmetic only)
                    if pool.reserve_a > 0 {
                        let p = pool.reserve_b
                            .saturating_mul(1_000_000)
                            / pool.reserve_a;
                        println!("  │ price A→B    : {}.{:06} B/A", p / 1_000_000, p % 1_000_000);
                    } else {
                        println!("  │ price A→B    : n/a (empty pool)");
                    }
                    // Constant-product k
                    match pool.reserve_a.checked_mul(pool.reserve_b) {
                        Some(k) => println!("  │ k = A×B      : {}", k),
                        None    => println!("  │ k = A×B      : overflow"),
                    }
                    // Per-LP backing
                    if pool.lp_supply > 0 {
                        let a_per = pool.reserve_a.saturating_mul(1_000_000) / pool.lp_supply;
                        let b_per = pool.reserve_b.saturating_mul(1_000_000) / pool.lp_supply;
                        println!("  │ A per LP×1e6 : {}", a_per);
                        println!("  │ B per LP×1e6 : {}", b_per);
                    } else {
                        println!("  │ A/B per LP   : n/a (no LP supply)");
                    }
                    println!("  └───────────────────────────────────────────────");
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: send an on-chain GetPoolInfo instruction and print the returned
// transaction signature.  The program will emit full pool state via msg!(),
// which is visible in the transaction logs (e.g. via solana logs or RPC).
// ─────────────────────────────────────────────────────────────────────────────
fn send_get_pool_info(
    client: &RpcClient,
    payer: &Keypair,
    program_id: &Pubkey,
    pool_pubkey: &Pubkey,
) {
    let ix_data = borsh::to_vec(&DexInstruction::GetPoolInfo)
        .expect("Failed to serialize GetPoolInfo");

    // Only one account needed: the pool (read-only, no signer)
    let ix = Instruction::new_with_bytes(
        *program_id,
        &ix_data,
        vec![AccountMeta::new_readonly(*pool_pubkey, false)],
    );

    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[payer], blockhash);

    match client.send_and_confirm_transaction(&tx) {
        Ok(sig) => println!("  [On-chain] GetPoolInfo tx: {} (check logs for pool state)", sig),
        Err(e)  => eprintln!("  [On-chain] GetPoolInfo failed: {:?}", e),
    }
}

#[tokio::main]
async fn main() {
    // Replace with your actual program ID from deployment
    let program_id = Pubkey::from_str("7ccZTYuUa3d3b82kMmfKG7jED2DYyNc2AJX3C6zHEqxB")
        .expect("Invalid program ID");

    // Connect to local cluster
    let rpc_url = String::from("http://localhost:8899");
    let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());

    // Generate a new keypair for paying fees and interacting
    let user = Keypair::new();

    // Request airdrop of 10 SOL for transaction fees
    println!("Requesting airdrop for user...");
    let airdrop_signature = client
        .request_airdrop(&user.pubkey(), 10_000_000_000)
        .expect("Failed to request airdrop");

    // Wait for airdrop confirmation
    loop {
        if client
            .confirm_transaction(&airdrop_signature)
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    println!("Airdrop confirmed for user: {}", user.pubkey());

    // --- SETUP TOKENS ---
    println!("\nSetting up Tokens...");
    let mint_authority = Keypair::new();
    let mint_a = create_mint(&client, &user, &mint_authority.pubkey(), 6);
    let mint_b = create_mint(&client, &user, &mint_authority.pubkey(), 6);

    let user_token_a = create_token_account(&client, &user, &mint_a.pubkey(), &user.pubkey());
    let user_token_b = create_token_account(&client, &user, &mint_b.pubkey(), &user.pubkey());

    // Mint some initial tokens to the user (1,000,000 tokens of each)
    let initial_mint_amount = 1_000_000_000_000;
    mint_to(
        &client,
        &user,
        &mint_a.pubkey(),
        &user_token_a.pubkey(),
        &mint_authority,
        initial_mint_amount,
    );
    mint_to(
        &client,
        &user,
        &mint_b.pubkey(),
        &user_token_b.pubkey(),
        &mint_authority,
        initial_mint_amount,
    );
    println!("Tokens A and B created, and minted to user wallets.");

    // --- SETUP DEX POOL ---
    println!("\nInitializing Pool...");
    let pool = Keypair::new();
    let pool_rent = client.get_minimum_balance_for_rent_exemption(POOL_SIZE).unwrap();

    // seeds must match processor.rs: [b"pool_authority", pool_account.key]
    let (pool_authority_pda, bump_value) = Pubkey::find_program_address(
        &[b"pool_authority", pool.pubkey().as_ref()],
        &program_id,
    );

    let vault_a = create_token_account(&client, &user, &mint_a.pubkey(), &pool_authority_pda);
    let vault_b = create_token_account(&client, &user, &mint_b.pubkey(), &pool_authority_pda);
    let lp_mint = create_mint(&client, &user, &pool_authority_pda, 6);
    let user_lp_token = create_token_account(&client, &user, &lp_mint.pubkey(), &user.pubkey());

    // Create the pool account explicitly
    let create_pool_ix = system_instruction::create_account(
        &user.pubkey(),
        &pool.pubkey(),
        pool_rent,
        POOL_SIZE as u64,
        &program_id,
    );
    let mut tx = Transaction::new_with_payer(&[create_pool_ix], Some(&user.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&user, &pool], blockhash);
    client.send_and_confirm_transaction(&tx).unwrap();

    // Serialize InitializePool instruction
    let init_instruction_data = borsh::to_vec(&DexInstruction::InitializePool {
        fee_numerator: 3,     // 0.3% fee
        fee_denominator: 1000,
        bump: bump_value,
    })
    .expect("Failed to serialize InitializePool instruction");

    // Account order from processor.rs:
    // 0. [writable] pool_account
    // 1. []         token_a_mint
    // 2. []         token_b_mint
    // 3. [writable] token_a_vault
    // 4. [writable] token_b_vault
    // 5. [writable] lp_mint
    // 6. []         pool_authority
    // 7. []         token_program
    let init_pool_ix = Instruction::new_with_bytes(
        program_id,
        &init_instruction_data,
        vec![
            AccountMeta::new(pool.pubkey(), false),           // 0 pool [writable]
            AccountMeta::new_readonly(mint_a.pubkey(), false), // 1 token_a_mint
            AccountMeta::new_readonly(mint_b.pubkey(), false), // 2 token_b_mint
            AccountMeta::new(vault_a.pubkey(), false),         // 3 token_a_vault [writable]
            AccountMeta::new(vault_b.pubkey(), false),         // 4 token_b_vault [writable]
            AccountMeta::new(lp_mint.pubkey(), false),         // 5 lp_mint [writable]
            AccountMeta::new_readonly(pool_authority_pda, false), // 6 pool_authority
            AccountMeta::new_readonly(spl_token::id(), false), // 7 token_program
        ],
    );

    let mut tx = Transaction::new_with_payer(&[init_pool_ix], Some(&user.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&user], blockhash);
    
    match client.send_and_confirm_transaction(&tx) {
        Ok(signature) => println!("Pool initialized! Transaction: {}", signature),
        Err(err) => {
            eprintln!("Failed to initialize pool: {:?}", err);
            return;
        }
    }

    // --- ADD LIQUIDITY ---
    println!("\nAdding Liquidity...");
    let add_liquidity_data = borsh::to_vec(&DexInstruction::AddLiquidity {
        token_a_amount: 100_000_000,
        token_b_amount: 100_000_000,
        min_lp_amount: 0, // No slippage protection for demo
    })
    .expect("Failed to serialize AddLiquidity instruction");

    // Account order from processor.rs:
    // 0. [signer]   user
    // 1. [writable] pool_account
    // 2. [writable] user_token_a
    // 3. [writable] token_a_vault
    // 4. [writable] user_token_b
    // 5. [writable] token_b_vault
    // 6. [writable] lp_mint
    // 7. [writable] user_lp_account
    // 8. []         pool_authority
    // 9. []         token_program
    let add_liquidity_ix = Instruction::new_with_bytes(
        program_id,
        &add_liquidity_data,
        vec![
            AccountMeta::new(user.pubkey(), true),             // 0 user [signer]
            AccountMeta::new(pool.pubkey(), false),            // 1 pool [writable]
            AccountMeta::new(user_token_a.pubkey(), false),    // 2 user_token_a [writable]
            AccountMeta::new(vault_a.pubkey(), false),         // 3 vault_a [writable]
            AccountMeta::new(user_token_b.pubkey(), false),    // 4 user_token_b [writable]
            AccountMeta::new(vault_b.pubkey(), false),         // 5 vault_b [writable]
            AccountMeta::new(lp_mint.pubkey(), false),         // 6 lp_mint [writable]
            AccountMeta::new(user_lp_token.pubkey(), false),   // 7 user_lp [writable]
            AccountMeta::new_readonly(pool_authority_pda, false), // 8 pool_authority
            AccountMeta::new_readonly(spl_token::id(), false), // 9 token_program
        ],
    );

    let mut tx = Transaction::new_with_payer(&[add_liquidity_ix], Some(&user.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&user], blockhash);
    
    match client.send_and_confirm_transaction(&tx) {
        Ok(signature) => println!("Liquidity added! Transaction: {}", signature),
        Err(err) => {
            eprintln!("Failed to add liquidity: {:?}", err);
            return;
        }
    }

    // --- GET POOL INFO (after AddLiquidity) ---
    println!("\n=== Pool Info after AddLiquidity ===");
    print_pool_info_local(&client, &pool.pubkey());
    send_get_pool_info(&client, &user, &program_id, &pool.pubkey());

    // --- SWAP ---
    println!("\nSwapping Tokens (Token A -> Token B)...");
    let swap_data = borsh::to_vec(&DexInstruction::Swap {
        amount_in: 10_000_000,
        min_amount_out: 0, // No slippage protection for demo
        a_to_b: true,      // Swapping token A for token B
    })
    .expect("Failed to serialize Swap instruction");

    // Account order from processor.rs:
    // 0. [signer]   user
    // 1. [writable] pool_account
    // 2. [writable] user_source_token (token A for a_to_b=true)
    // 3. [writable] pool_source_vault (vault A)
    // 4. [writable] pool_dest_vault   (vault B)
    // 5. [writable] user_dest_token   (token B)
    // 6. []         pool_authority
    // 7. []         token_program
    let swap_ix = Instruction::new_with_bytes(
        program_id,
        &swap_data,
        vec![
            AccountMeta::new(user.pubkey(), true),             // 0 user [signer]
            AccountMeta::new(pool.pubkey(), false),            // 1 pool [writable]
            AccountMeta::new(user_token_a.pubkey(), false),    // 2 user source (A) [writable]
            AccountMeta::new(vault_a.pubkey(), false),         // 3 pool source vault (A) [writable]
            AccountMeta::new(vault_b.pubkey(), false),         // 4 pool dest vault (B) [writable]
            AccountMeta::new(user_token_b.pubkey(), false),    // 5 user dest (B) [writable]
            AccountMeta::new_readonly(pool_authority_pda, false), // 6 pool authority
            AccountMeta::new_readonly(spl_token::id(), false), // 7 token_program
        ],
    );

    let mut tx = Transaction::new_with_payer(&[swap_ix], Some(&user.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&user], blockhash);
    
    match client.send_and_confirm_transaction(&tx) {
        Ok(signature) => println!("Swap completed! Transaction: {}", signature),
        Err(err) => {
            eprintln!("Failed to perform swap: {:?}", err);
            return;
        }
    }

    // --- GET POOL INFO (after Swap) ---
    println!("\n=== Pool Info after Swap ===");
    print_pool_info_local(&client, &pool.pubkey());
    send_get_pool_info(&client, &user, &program_id, &pool.pubkey());

    // --- REMOVE LIQUIDITY ---
    println!("\nRemoving Liquidity...");
    let remove_liquidity_data = borsh::to_vec(&DexInstruction::RemoveLiquidity {
        lp_amount: 50_000_000, // Remove half of LP tokens
        min_token_a: 0,
        min_token_b: 0,
    })
    .expect("Failed to serialize RemoveLiquidity instruction");

    // Account order from processor.rs:
    // 0. [signer]   user
    // 1. [writable] pool_account
    // 2. [writable] user_lp_account
    // 3. [writable] lp_mint
    // 4. [writable] token_a_vault
    // 5. [writable] user_token_a
    // 6. [writable] token_b_vault
    // 7. [writable] user_token_b
    // 8. []         pool_authority
    // 9. []         token_program
    let remove_liquidity_ix = Instruction::new_with_bytes(
        program_id,
        &remove_liquidity_data,
        vec![
            AccountMeta::new(user.pubkey(), true),             // 0 user [signer]
            AccountMeta::new(pool.pubkey(), false),            // 1 pool [writable]
            AccountMeta::new(user_lp_token.pubkey(), false),   // 2 user_lp [writable]
            AccountMeta::new(lp_mint.pubkey(), false),         // 3 lp_mint [writable]
            AccountMeta::new(vault_a.pubkey(), false),         // 4 vault_a [writable]
            AccountMeta::new(user_token_a.pubkey(), false),    // 5 user_token_a [writable]
            AccountMeta::new(vault_b.pubkey(), false),         // 6 vault_b [writable]
            AccountMeta::new(user_token_b.pubkey(), false),    // 7 user_token_b [writable]
            AccountMeta::new_readonly(pool_authority_pda, false), // 8 pool_authority
            AccountMeta::new_readonly(spl_token::id(), false), // 9 token_program
        ],
    );

    let mut tx = Transaction::new_with_payer(&[remove_liquidity_ix], Some(&user.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&user], blockhash);
    
    match client.send_and_confirm_transaction(&tx) {
        Ok(signature) => println!("Liquidity removed! Transaction: {}", signature),
        Err(err) => {
            eprintln!("Failed to remove liquidity: {:?}", err);
        }
    }

    // --- GET POOL INFO (after RemoveLiquidity) ---
    println!("\n=== Pool Info after RemoveLiquidity ===");
    print_pool_info_local(&client, &pool.pubkey());
    send_get_pool_info(&client, &user, &program_id, &pool.pubkey());

    println!("\nDEX Demo Client execution completed successfully!");
}