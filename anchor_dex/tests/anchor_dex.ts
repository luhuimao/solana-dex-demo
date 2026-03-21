import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AnchorDex } from "../target/types/anchor_dex";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { assert } from "chai";

describe("anchor_dex", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.AnchorDex as Program<AnchorDex>;
  const connection = provider.connection;
  const wallet = provider.wallet as anchor.Wallet;

  // Keypairs
  const poolKp       = anchor.web3.Keypair.generate();
  const vaultAKp     = anchor.web3.Keypair.generate();
  const vaultBKp     = anchor.web3.Keypair.generate();
  const lpMintKp     = anchor.web3.Keypair.generate();

  // Pubkeys derived later
  let mintA: anchor.web3.PublicKey;
  let mintB: anchor.web3.PublicKey;
  let userTokenA: anchor.web3.PublicKey;
  let userTokenB: anchor.web3.PublicKey;
  let userLp: anchor.web3.PublicKey;
  let poolAuthority: anchor.web3.PublicKey;
  let poolAuthorityBump: number;

  const FEE_NUMERATOR   = new anchor.BN(3);
  const FEE_DENOMINATOR = new anchor.BN(1000);
  const MINT_AMOUNT     = 1_000_000_000_000n; // 1e12 raw

  // ─── helpers ────────────────────────────────────────────────────────────
  async function balance(pubkey: anchor.web3.PublicKey): Promise<bigint> {
    return (await getAccount(connection, pubkey)).amount;
  }

  // ─── setup ──────────────────────────────────────────────────────────────
  before(async () => {
    // Derive pool_authority PDA
    [poolAuthority, poolAuthorityBump] =
      anchor.web3.PublicKey.findProgramAddressSync(
        [Buffer.from("pool_authority"), poolKp.publicKey.toBuffer()],
        program.programId
      );

    // Create mints (authority = wallet so we can mintTo)
    mintA = await createMint(connection, wallet.payer, wallet.publicKey, null, 6);
    mintB = await createMint(connection, wallet.payer, wallet.publicKey, null, 6);

    // User token accounts
    userTokenA = await createAccount(connection, wallet.payer, mintA, wallet.publicKey);
    userTokenB = await createAccount(connection, wallet.payer, mintB, wallet.publicKey);

    // Mint tokens to user
    await mintTo(connection, wallet.payer, mintA, userTokenA, wallet.payer, MINT_AMOUNT);
    await mintTo(connection, wallet.payer, mintB, userTokenB, wallet.payer, MINT_AMOUNT);
  });

  // ─────────────────────────────────────────────────────────────────────────
  it("InitializePool", async () => {
    await program.methods
      .initializePool(FEE_NUMERATOR, FEE_DENOMINATOR)
      .accountsPartial({
        pool:          poolKp.publicKey,
        tokenAMint:    mintA,
        tokenBMint:    mintB,
        tokenAVault:   vaultAKp.publicKey,
        tokenBVault:   vaultBKp.publicKey,
        lpMint:        lpMintKp.publicKey,
        poolAuthority,
        user:          wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram:  TOKEN_PROGRAM_ID,
        rent:          anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([poolKp, vaultAKp, vaultBKp, lpMintKp])
      .rpc();

    const pool = await program.account.pool.fetch(poolKp.publicKey);
    assert.ok(pool.tokenAMint.equals(mintA));
    assert.ok(pool.tokenBMint.equals(mintB));
    assert.equal(pool.feeNumerator.toNumber(), 3);
    assert.equal(pool.feeDenominator.toNumber(), 1000);
    assert.equal(pool.reserveA.toNumber(), 0);
    assert.equal(pool.reserveB.toNumber(), 0);
    assert.equal(pool.lpSupply.toNumber(), 0);

    // Create user LP token account (needs lp mint to exist first)
    userLp = await createAccount(connection, wallet.payer, lpMintKp.publicKey, wallet.publicKey);

    console.log("  ✓ Pool initialized, fee=3/1000");
  });

  // ─────────────────────────────────────────────────────────────────────────
  it("AddLiquidity", async () => {
    const amtA = new anchor.BN(100_000_000);
    const amtB = new anchor.BN(100_000_000);

    await program.methods
      .addLiquidity(amtA, amtB, new anchor.BN(0))
      .accountsPartial({
        pool:         poolKp.publicKey,
        tokenAMint:   mintA,
        tokenBMint:   mintB,
        userTokenA,
        tokenAVault:  vaultAKp.publicKey,
        userTokenB,
        tokenBVault:  vaultBKp.publicKey,
        lpMint:       lpMintKp.publicKey,
        userLp,
        poolAuthority,
        user:         wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const pool = await program.account.pool.fetch(poolKp.publicKey);
    assert.equal(pool.reserveA.toNumber(), 100_000_000);
    assert.equal(pool.reserveB.toNumber(), 100_000_000);
    // LP = sqrt(100e6 * 100e6) = 100e6
    assert.equal(pool.lpSupply.toNumber(), 100_000_000);
    assert.equal(Number(await balance(userLp)), 100_000_000);

    console.log(`  ✓ Liquidity added | reserveA=${pool.reserveA} reserveB=${pool.reserveB} LP=${pool.lpSupply}`);
  });

  // ─────────────────────────────────────────────────────────────────────────
  it("Swap A→B", async () => {
    const poolBefore = await program.account.pool.fetch(poolKp.publicKey);
    const kBefore = BigInt(poolBefore.reserveA.toNumber()) * BigInt(poolBefore.reserveB.toNumber());

    await program.methods
      .swap(new anchor.BN(10_000_000), new anchor.BN(0), true)
      .accountsPartial({
        pool:           poolKp.publicKey,
        tokenAMint:     mintA,
        tokenBMint:     mintB,
        userSource:     userTokenA,
        poolSourceVault: vaultAKp.publicKey,
        poolDestVault:  vaultBKp.publicKey,
        userDest:       userTokenB,
        poolAuthority,
        user:           wallet.publicKey,
        tokenProgram:   TOKEN_PROGRAM_ID,
      })
      .rpc();

    const pool = await program.account.pool.fetch(poolKp.publicKey);
    const kAfter = BigInt(pool.reserveA.toNumber()) * BigInt(pool.reserveB.toNumber());

    assert.equal(pool.reserveA.toNumber(), 110_000_000, "reserveA should increase by amount_in");
    assert.ok(pool.reserveB.toNumber() < 100_000_000, "reserveB should decrease");
    assert.ok(kAfter >= kBefore, "k must not decrease after swap");

    console.log(`  ✓ Swap | reserveA=${pool.reserveA} reserveB=${pool.reserveB} k(after)=${kAfter}`);
  });

  // ─────────────────────────────────────────────────────────────────────────
  it("RemoveLiquidity", async () => {
    const lpBalance  = await balance(userLp);
    const halfLp     = new anchor.BN(lpBalance.toString()).divn(2);
    const poolBefore = await program.account.pool.fetch(poolKp.publicKey);

    await program.methods
      .removeLiquidity(halfLp, new anchor.BN(0), new anchor.BN(0))
      .accountsPartial({
        pool:         poolKp.publicKey,
        tokenAMint:   mintA,
        tokenBMint:   mintB,
        userLp,
        lpMint:       lpMintKp.publicKey,
        tokenAVault:  vaultAKp.publicKey,
        userTokenA,
        tokenBVault:  vaultBKp.publicKey,
        userTokenB,
        poolAuthority,
        user:         wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const pool = await program.account.pool.fetch(poolKp.publicKey);
    // Reserves should be roughly halved
    assert.ok(
      pool.reserveA.toNumber() < poolBefore.reserveA.toNumber(),
      "reserveA should decrease"
    );
    assert.ok(
      pool.reserveB.toNumber() < poolBefore.reserveB.toNumber(),
      "reserveB should decrease"
    );
    assert.equal(pool.lpSupply.toNumber(), 50_000_000, "LP supply halved");

    console.log(`  ✓ Remove liquidity | reserveA=${pool.reserveA} reserveB=${pool.reserveB} LP=${pool.lpSupply}`);
  });

  // ─────────────────────────────────────────────────────────────────────────
  it("GetPoolInfo", async () => {
    const tx = await program.methods
      .getPoolInfo()
      .accountsPartial({ pool: poolKp.publicKey })
      .rpc();

    console.log(`  ✓ GetPoolInfo tx: ${tx} (check logs for pool state)`);
    // Verify pool state is still intact (read-only instruction)
    const pool = await program.account.pool.fetch(poolKp.publicKey);
    assert.ok(pool.reserveA.toNumber() > 0);
    assert.ok(pool.reserveB.toNumber() > 0);
  });
});
