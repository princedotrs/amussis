use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("2ihBJmUBY5WyaunnKMTdMhbZRg69Yn1sDeVNVQRfx5VC");

pub const VAULT_SEED: &[u8] = b"vault";

#[program]
pub mod vault {
    use super::*;

    /// Create the signer's own vault. `locked_duration` is in seconds.
    pub fn init(ctx: Context<Init>, locked_duration: i64) -> Result<()> {
        require!(locked_duration >= 0, VaultError::InvalidDuration);
        let v = &mut ctx.accounts.vault;
        v.owner = ctx.accounts.user.key();
        v.amount = 0;
        v.locked_duration = locked_duration;
        v.unlock_time = 0;
        v.bump = ctx.bumps.vault;
        Ok(())
    }

    /// Deposit SOL. Every deposit restarts the lock: unlock = now + locked_duration.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                Transfer {
                    from: ctx.accounts.user.to_account_info(),
                    to: ctx.accounts.vault.to_account_info(),
                },
            ),
            amount,
        )?;
        let v = &mut ctx.accounts.vault;
        v.amount = v.amount.checked_add(amount).ok_or(VaultError::Overflow)?;
        v.unlock_time = Clock::get()?
            .unix_timestamp
            .checked_add(v.locked_duration)
            .ok_or(VaultError::Overflow)?;
        Ok(())
    }

    /// Withdraw the full deposited balance once the lock has expired.
    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        let v = &mut ctx.accounts.vault;
        require!(v.amount > 0, VaultError::ZeroAmount);
        require!(
            Clock::get()?.unix_timestamp >= v.unlock_time,
            VaultError::StillLocked
        );
        let amount = std::mem::take(&mut v.amount);
        // The vault is owned by this program, so move lamports directly.
        ctx.accounts.vault.sub_lamports(amount)?;
        ctx.accounts.user.add_lamports(amount)?;
        Ok(())
    }

    /// Close an empty vault and refund its rent to the owner.
    pub fn close(_ctx: Context<Close>) -> Result<()> {
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub owner: Pubkey,
    pub amount: u64,
    pub locked_duration: i64,
    pub unlock_time: i64,
    pub bump: u8,
}

#[derive(Accounts)]
pub struct Init<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        init, payer = user, space = 8 + Vault::INIT_SPACE,
        seeds = [VAULT_SEED, user.key().as_ref()], bump
    )]
    pub vault: Account<'info, Vault>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut, constraint = vault.owner == user.key() @ VaultError::NotOwner,
        seeds = [VAULT_SEED, user.key().as_ref()], bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut, constraint = vault.owner == user.key() @ VaultError::NotOwner,
        seeds = [VAULT_SEED, user.key().as_ref()], bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,
}

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut, close = user,
        constraint = vault.owner == user.key() @ VaultError::NotOwner,
        constraint = vault.amount == 0 @ VaultError::NotEmpty,
        seeds = [VAULT_SEED, user.key().as_ref()], bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,
}

#[error_code]
pub enum VaultError {
    #[msg("Signer is not the vault owner")]
    NotOwner,
    #[msg("Vault is still locked")]
    StillLocked,
    #[msg("Vault must be empty to close")]
    NotEmpty,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Invalid lock duration")]
    InvalidDuration,
    #[msg("Arithmetic overflow")]
    Overflow,
}
