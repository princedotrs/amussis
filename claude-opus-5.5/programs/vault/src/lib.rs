use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("85GHK378iHbxfFCzzf9b6Sp82wzyBtEqqj1d72WLRyNV");

#[program]
pub mod vault {
    use super::*;

    /// Creates the user's state + SOL vault and sets the lock duration (seconds).
    pub fn init(ctx: Context<Init>, locked_duration: i64) -> Result<()> {
        require!(locked_duration > 0, VaultError::InvalidDuration);
        let state = &mut ctx.accounts.state;
        state.owner = ctx.accounts.user.key();
        state.locked_duration = locked_duration;
        state.state_bump = ctx.bumps.state;
        state.vault_bump = ctx.bumps.vault;
        // Fund the vault with its rent-exempt minimum so later transfers never fail on rent.
        let rent = Rent::get()?.minimum_balance(0);
        pay(&ctx.accounts.system_program, &ctx.accounts.user, &ctx.accounts.vault, rent, None)
    }

    /// Adds SOL to the vault. Every deposit restarts the lock timer.
    pub fn deposit(ctx: Context<Manage>, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::InvalidAmount);
        let a = &ctx.accounts;
        pay(&a.system_program, &a.user, &a.vault, amount, None)?;
        let state = &mut ctx.accounts.state;
        state.amount = state.amount.checked_add(amount).ok_or(VaultError::Overflow)?;
        state.last_deposit_ts = Clock::get()?.unix_timestamp;
        Ok(())
    }

    /// Withdraws `amount` lamports, only once the lock has expired.
    pub fn withdraw(ctx: Context<Manage>, amount: u64) -> Result<()> {
        let state = &ctx.accounts.state;
        require!(amount > 0, VaultError::InvalidAmount);
        require!(amount <= state.amount, VaultError::InsufficientFunds);
        state.check_unlocked()?;
        let user_key = ctx.accounts.user.key();
        let seeds: &[&[u8]] = &[b"vault", user_key.as_ref(), &[state.vault_bump]];
        let a = &ctx.accounts;
        pay(&a.system_program, &a.vault, &a.user, amount, Some(seeds))?;
        ctx.accounts.state.amount -= amount;
        Ok(())
    }

    /// Empties the vault back to the user and closes the state account (refunding rent).
    pub fn close(ctx: Context<Close>) -> Result<()> {
        let state = &ctx.accounts.state;
        if state.amount > 0 { state.check_unlocked()?; } // an empty vault can always close
        let user_key = ctx.accounts.user.key();
        let seeds: &[&[u8]] = &[b"vault", user_key.as_ref(), &[state.vault_bump]];
        let a = &ctx.accounts;
        pay(&a.system_program, &a.vault, &a.user, a.vault.lamports(), Some(seeds))
    }
}

/// System-program transfer; `seeds` is set when the vault PDA is the payer.
fn pay<'info>(
    system_program: &Program<'info, System>,
    from: &impl ToAccountInfo<'info>,
    to: &impl ToAccountInfo<'info>,
    lamports: u64,
    seeds: Option<&[&[u8]]>,
) -> Result<()> {
    let accounts = Transfer { from: from.to_account_info(), to: to.to_account_info() };
    match seeds {
        Some(s) => transfer(CpiContext::new_with_signer(system_program.key(), accounts, &[s]), lamports),
        None => transfer(CpiContext::new(system_program.key(), accounts), lamports),
    }
}

#[account]
#[derive(InitSpace)]
pub struct VaultState {
    pub owner: Pubkey,
    pub amount: u64,
    pub locked_duration: i64,
    pub last_deposit_ts: i64,
    pub state_bump: u8,
    pub vault_bump: u8,
}

impl VaultState {
    fn check_unlocked(&self) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        require!(now >= self.last_deposit_ts + self.locked_duration, VaultError::StillLocked);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Init<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(init, payer = user, space = 8 + VaultState::INIT_SPACE,
        seeds = [b"state", user.key().as_ref()], bump)]
    pub state: Account<'info, VaultState>,
    #[account(mut, seeds = [b"vault", user.key().as_ref()], bump)]
    pub vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Shared by `deposit` and `withdraw`.
#[derive(Accounts)]
pub struct Manage<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, constraint = state.owner == user.key() @ VaultError::NotOwner,
        seeds = [b"state", user.key().as_ref()], bump = state.state_bump)]
    pub state: Account<'info, VaultState>,
    #[account(mut, seeds = [b"vault", user.key().as_ref()], bump = state.vault_bump)]
    pub vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, close = user, constraint = state.owner == user.key() @ VaultError::NotOwner,
        seeds = [b"state", user.key().as_ref()], bump = state.state_bump)]
    pub state: Account<'info, VaultState>,
    #[account(mut, seeds = [b"vault", user.key().as_ref()], bump = state.vault_bump)]
    pub vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum VaultError { InvalidDuration, InvalidAmount, StillLocked, InsufficientFunds, NotOwner, Overflow }
