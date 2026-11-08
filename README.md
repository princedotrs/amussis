# Time-locked SOL Vault

A small Solana program (Anchor, Rust) where each user gets a personal vault. You can deposit SOL any time, but you can only withdraw after a minimum lock time has passed since your last deposit.

Rust size: `programs/vault/src/lib.rs` (131 lines) + `programs/vault/tests/vault.rs` (65 lines) = **196 lines**.

## Build and run

```bash
cargo build                # builds the program crate on the host
cargo build-sbf            # builds the on-chain binary -> target/deploy/vault.so (Solana CLI 2.x/3.x)
cargo test                 # runs the program inside LiteSVM (a local Solana VM)
```

`cargo test` loads `target/deploy/vault.so`, so run `cargo build-sbf` first.

Tested with rustc 1.98, anchor-lang 1.2.2, Solana CLI / cargo-build-sbf 3.1.10, litesvm 0.18.2.

## Instructions

| Instruction | What it does |
|---|---|
| `init(locked_duration)` | Creates the user's `state` PDA and funds the `vault` PDA with its rent-exempt minimum. `locked_duration` is in seconds and must be > 0. |
| `deposit(amount)` | Moves `amount` lamports from the user into the vault, adds it to `state.amount` and restarts the lock timer. |
| `withdraw(amount)` | Sends `amount` lamports back to the user. Only allowed when `now >= last_deposit_ts + locked_duration` and `amount <= state.amount`. |
| `close()` | Sends everything left in the vault back to the user and closes the state account, which refunds its rent. If funds are still inside, the lock has to have expired. |

### Accounts

- `state`: PDA with seeds `["state", user]`, owned by the program. It stores:
  `owner, amount, locked_duration, last_deposit_ts, state_bump, vault_bump`
- `vault`: PDA with seeds `["vault", user]`, a system-owned account that holds the SOL.

## How I described it to the agents (main prompts)

<!-- TODO: paste your real prompts here. Prompt 1 is the one used with Claude Code. -->

1. *"I want to build a simple vault program where users can deposit SOL with constraints like min time and they can only withdraw after that time is passed. So we are gonna have a simple vault state with amount, locked_duration and bumps. We are gonna have instructions like init, deposit, withdraw and close. Every user can init their own vault, so the seed would contain a simple vault string and the user's pubkey, and we check the owner in the other instructions."*
2. TODO

## Models I tried and how they differed

<!-- TODO: fill in from your own runs with the other model(s). -->

| Model | Tool | What happened |
|---|---|---|
| Claude Opus 5.5 | Claude Code | Wrote the program and a LiteSVM test. It added a `last_deposit_ts` field, because a lock duration alone doesn't say when the lock started. It pre-funds the vault with rent so small deposits and partial withdrawals don't fail the system program's rent check. It also requires the lock to have expired before `close`, since otherwise close would be a way around the lock. |
| TODO | TODO | TODO |

## How the program works (in my own words)

<!-- TODO: the assignment asks for YOUR words. These notes are a starting point; rewrite them. -->

- **One vault per user.** Both PDAs are derived from the signer's pubkey. If someone else signs, the program derives *their* addresses, which don't match this vault, so they can't touch it. On top of that, the program checks `state.owner == user`.
- **Where the SOL sits.** It lives in the `vault` PDA. The program has no private key for it, so to move SOL out it calls the System Program's `transfer` and signs with the PDA seeds `["vault", user, vault_bump]`. Deposits are plain transfers signed by the user.
- **The lock.** Each deposit stores `Clock::unix_timestamp`. `withdraw` and `close` fail with `StillLocked` until `locked_duration` seconds have passed.
- **Rent.** During `init` the vault gets the rent-exempt minimum for a 0-byte account, so later transfers never leave it in an invalid state. `close` returns that too.
- **Test.** The test initialises a vault with a 60 s lock and deposits 2 SOL. It checks that withdraw and close fail while locked, moves the clock forward 61 s, checks that over-withdrawing fails, withdraws 1 SOL, checks that a second wallet can't withdraw, then closes the vault and checks the user got everything back except fees.
