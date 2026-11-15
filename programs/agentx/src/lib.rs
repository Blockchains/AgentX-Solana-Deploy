//! AgentX Protocol — Solana program
//!
//! This program manages the lifecycle of autonomous AI agents on-chain:
//! - Registration: agents claim a PDA identity tied to their owner wallet
//! - Execution: oracle-signed task results are committed on-chain
//! - Rewards: agents accumulate AGX token rewards for completed tasks
//! - Governance: agents participate in protocol voting (future)

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("7cnrBzeFJVWR54PXadfDzE9ZWy975TaCTfBTGVL5wi5K");

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum length of an agent name (UTF-8 bytes)
pub const MAX_NAME_LEN: usize = 64;

/// Maximum length of model identifier string
pub const MAX_MODEL_LEN: usize = 32;

/// Maximum length of task result payload stored on-chain (summary only)
pub const MAX_TASK_RESULT_LEN: usize = 256;

/// Lamports required as rent-exempt deposit for AgentAccount
pub const AGENT_ACCOUNT_SIZE: usize = 8   // discriminator
    + 32   // owner pubkey
    + 8    // agent_id [u8; 8]
    + 64   // name
    + 32   // model
    + 8    // created_at
    + 8    // updated_at
    + 1    // status
    + 8    // tasks_completed
    + 8    // tasks_failed
    + 8    // total_rewards
    + 256; // padding for future fields


// ---------------------------------------------------------------------------
// Agent status enum
// ---------------------------------------------------------------------------

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentStatus {
    /// Agent is registered but not yet active
    Inactive = 0,
    /// Agent is actively processing tasks
    Active = 1,
    /// Agent has been paused by its owner
    Paused = 2,
    /// Agent has been permanently deactivated
    Deactivated = 3,
}

impl Default for AgentStatus {
    fn default() -> Self {
        AgentStatus::Inactive
    }
}


// ---------------------------------------------------------------------------
// Account structures
// ---------------------------------------------------------------------------

/// Per-agent PDA account.
/// Seeds: ["agent", owner.key(), agent_id]
#[account]
#[derive(Default)]
pub struct AgentAccount {
    /// Wallet that owns and controls this agent
    pub owner: Pubkey,

    /// 8-byte unique identifier (matches off-chain UUID truncation)
    pub agent_id: [u8; 8],

    /// Human-readable agent name (max MAX_NAME_LEN chars)
    pub name: String,

    /// LLM model backing this agent (e.g. "gpt-4o")
    pub model: String,

    /// Unix timestamp of registration
    pub created_at: i64,

    /// Unix timestamp of last state update
    pub updated_at: i64,

    /// Current lifecycle status
    pub status: AgentStatus,

    /// Number of tasks successfully completed and verified
    pub tasks_completed: u64,

    /// Number of tasks that failed verification
    pub tasks_failed: u64,

    /// Cumulative AGX reward lamports earned (before claim)
    pub total_rewards: u64,
}

/// Global registry account.
/// Seeds: ["registry"]
#[account]
pub struct RegistryAccount {
    /// Protocol authority (multisig in production)
    pub authority: Pubkey,

    /// Total agents ever registered
    pub total_agents: u64,

    /// Total tasks ever executed
    pub total_tasks: u64,

    /// AGX mint address
    pub agx_mint: Pubkey,

    /// Whether new registrations are paused
    pub registration_paused: bool,
}

/// Task execution record PDA.
/// Seeds: ["task", agent_account.key(), task_nonce]
#[account]
pub struct TaskAccount {
    pub agent: Pubkey,
    pub task_nonce: u64,
    pub submitted_at: i64,
    pub result_hash: [u8; 32],   // SHA-256 of full result (stored off-chain)
    pub result_summary: String,  // Short on-chain summary (≤256 chars)
    pub reward_lamports: u64,
    pub verified: bool,
}


// ---------------------------------------------------------------------------
// Instruction contexts
// ---------------------------------------------------------------------------

/// One-time lab/admin setup: create the registry PDA that register_agent requires.
/// Upstream ships no initialize instruction; this is a Sol.College demonstration patch.
#[derive(Accounts)]
pub struct InitializeRegistry<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8 + 32 + 1,
        seeds = [b"registry"],
        bump
    )]
    pub registry: Account<'info, RegistryAccount>,

    /// CHECK: AGX mint is recorded for later claim_reward; not validated beyond being a pubkey.
    pub agx_mint: AccountInfo<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(agent_id: [u8; 8], name: String, model: String)]
pub struct RegisterAgent<'info> {
    #[account(
        init,
        payer = owner,
        space = AGENT_ACCOUNT_SIZE,
        seeds = [b"agent", owner.key().as_ref(), &agent_id],
        bump
    )]
    pub agent_account: Account<'info, AgentAccount>,

    #[account(
        mut,
        seeds = [b"registry"],
        bump,
        constraint = !registry.registration_paused @ AgentXError::RegistrationPaused
    )]
    pub registry: Account<'info, RegistryAccount>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(task_nonce: u64)]
pub struct ExecuteTask<'info> {
    #[account(
        init,
        payer = oracle,
        space = 8 + 32 + 8 + 8 + 32 + MAX_TASK_RESULT_LEN + 8 + 1 + 64,
        seeds = [b"task", agent_account.key().as_ref(), &task_nonce.to_le_bytes()],
        bump
    )]
    pub task_account: Account<'info, TaskAccount>,

    #[account(
        mut,
        seeds = [b"agent", agent_account.owner.as_ref(), &agent_account.agent_id],
        bump,
        constraint = agent_account.status == AgentStatus::Active @ AgentXError::AgentNotActive
    )]
    pub agent_account: Account<'info, AgentAccount>,

    #[account(
        mut,
        seeds = [b"registry"],
        bump
    )]
    pub registry: Account<'info, RegistryAccount>,

    /// Oracle must be the protocol-designated oracle signer
    #[account(
        mut,
        constraint = oracle.key() == registry.authority @ AgentXError::Unauthorized
    )]
    pub oracle: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateState<'info> {
    #[account(
        mut,
        seeds = [b"agent", owner.key().as_ref(), &agent_account.agent_id],
        bump,
        has_one = owner @ AgentXError::Unauthorized
    )]
    pub agent_account: Account<'info, AgentAccount>,

    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct ClaimReward<'info> {
    #[account(
        mut,
        seeds = [b"agent", owner.key().as_ref(), &agent_account.agent_id],
        bump,
        has_one = owner @ AgentXError::Unauthorized
    )]
    pub agent_account: Account<'info, AgentAccount>,

    #[account(mut)]
    pub reward_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub owner_token_account: Account<'info, TokenAccount>,

    #[account(
        seeds = [b"registry"],
        bump
    )]
    pub registry: Account<'info, RegistryAccount>,

    pub owner: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct DeactivateAgent<'info> {
    #[account(
        mut,
        seeds = [b"agent", owner.key().as_ref(), &agent_account.agent_id],
        bump,
        has_one = owner @ AgentXError::Unauthorized
    )]
    pub agent_account: Account<'info, AgentAccount>,

    pub owner: Signer<'info>,
}


// ---------------------------------------------------------------------------
// Program entrypoint
// ---------------------------------------------------------------------------

#[program]
pub mod agentx {
    use super::*;

    /// Initialise the global registry PDA (seeds ["registry"]).
    /// Lab patch: upstream has no initialize instruction, but every other
    /// instruction requires this account to already exist.
    pub fn initialize_registry(ctx: Context<InitializeRegistry>) -> Result<()> {
        let registry = &mut ctx.accounts.registry;
        registry.authority = ctx.accounts.authority.key();
        registry.total_agents = 0;
        registry.total_tasks = 0;
        registry.agx_mint = ctx.accounts.agx_mint.key();
        registry.registration_paused = false;
        msg!("AgentX: registry initialised, authority={}", registry.authority);
        Ok(())
    }

    /// Register a new AI agent on-chain.
    ///
    /// Creates an `AgentAccount` PDA seeded by [owner, agent_id].
    /// The agent starts with status `Inactive`; call `update_state` to activate.
    pub fn register_agent(
        ctx: Context<RegisterAgent>,
        agent_id: [u8; 8],
        name: String,
        model: String,
    ) -> Result<()> {
        require!(name.len() <= MAX_NAME_LEN, AgentXError::NameTooLong);
        require!(model.len() <= MAX_MODEL_LEN, AgentXError::ModelTooLong);

        let clock = Clock::get()?;
        let agent = &mut ctx.accounts.agent_account;
        let registry = &mut ctx.accounts.registry;

        agent.owner = ctx.accounts.owner.key();
        agent.agent_id = agent_id;
        agent.name = name.clone();
        agent.model = model;
        agent.created_at = clock.unix_timestamp;
        agent.updated_at = clock.unix_timestamp;
        agent.status = AgentStatus::Inactive;
        agent.tasks_completed = 0;
        agent.tasks_failed = 0;
        agent.total_rewards = 0;

        registry.total_agents = registry.total_agents.checked_add(1).unwrap();

        emit!(AgentRegisteredEvent {
            owner: ctx.accounts.owner.key(),
            agent_id,
            name,
            timestamp: clock.unix_timestamp,
        });

        msg!("AgentX: agent registered, owner={}", ctx.accounts.owner.key());
        Ok(())
    }

    /// Submit a verified task execution result from an oracle.
    ///
    /// The oracle provides a SHA-256 hash of the full result (stored off-chain
    /// on Arweave/IPFS) and a short summary stored on-chain.
    /// On success, rewards are credited to the agent.
    pub fn execute_task(
        ctx: Context<ExecuteTask>,
        task_nonce: u64,
        result_hash: [u8; 32],
        result_summary: String,
        reward_lamports: u64,
        success: bool,
    ) -> Result<()> {
        require!(
            result_summary.len() <= MAX_TASK_RESULT_LEN,
            AgentXError::ResultTooLong
        );

        let clock = Clock::get()?;
        let task = &mut ctx.accounts.task_account;
        let agent = &mut ctx.accounts.agent_account;
        let registry = &mut ctx.accounts.registry;

        task.agent = agent.key();
        task.task_nonce = task_nonce;
        task.submitted_at = clock.unix_timestamp;
        task.result_hash = result_hash;
        task.result_summary = result_summary;
        task.reward_lamports = reward_lamports;
        task.verified = success;

        agent.updated_at = clock.unix_timestamp;
        registry.total_tasks = registry.total_tasks.checked_add(1).unwrap();

        if success {
            agent.tasks_completed = agent.tasks_completed.checked_add(1).unwrap();
            agent.total_rewards = agent.total_rewards.checked_add(reward_lamports).unwrap();
        } else {
            agent.tasks_failed = agent.tasks_failed.checked_add(1).unwrap();
        }

        emit!(TaskExecutedEvent {
            agent: agent.key(),
            task_nonce,
            success,
            reward_lamports,
            timestamp: clock.unix_timestamp,
        });

        msg!("AgentX: task {} executed, success={}", task_nonce, success);
        Ok(())
    }

    /// Update the agent's lifecycle status.
    ///
    /// Only the agent's owner can call this instruction.
    /// Cannot reactivate a Deactivated agent.
    pub fn update_state(ctx: Context<UpdateState>, new_status: AgentStatus) -> Result<()> {
        let agent = &mut ctx.accounts.agent_account;
        require!(
            agent.status != AgentStatus::Deactivated,
            AgentXError::AgentDeactivated
        );

        let clock = Clock::get()?;
        agent.status = new_status;
        agent.updated_at = clock.unix_timestamp;

        msg!("AgentX: agent state updated to {:?}", new_status);
        Ok(())
    }

    /// Claim accumulated AGX token rewards for the agent.
    ///
    /// Transfers `agent.total_rewards` from the protocol reward vault
    /// to the owner's token account, then resets the balance.
    pub fn claim_reward(ctx: Context<ClaimReward>) -> Result<()> {
        let agent = &mut ctx.accounts.agent_account;
        let amount = agent.total_rewards;

        require!(amount > 0, AgentXError::NoRewardsToClaim);

        // Reset before transfer to prevent re-entrancy
        agent.total_rewards = 0;

        let cpi_accounts = Transfer {
            from: ctx.accounts.reward_vault.to_account_info(),
            to: ctx.accounts.owner_token_account.to_account_info(),
            authority: ctx.accounts.registry.to_account_info(),
        };
        let seeds = &[b"registry".as_ref(), &[ctx.bumps.registry]];
        let signer_seeds = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token::transfer(cpi_ctx, amount)?;

        emit!(RewardClaimedEvent {
            owner: ctx.accounts.owner.key(),
            amount,
        });

        msg!("AgentX: {} AGX reward claimed by {}", amount, ctx.accounts.owner.key());
        Ok(())
    }

    /// Permanently deactivate an agent.
    ///
    /// This action is irreversible. The AgentAccount can be closed
    /// in a subsequent instruction to reclaim rent.
    pub fn deactivate_agent(ctx: Context<DeactivateAgent>) -> Result<()> {
        let agent = &mut ctx.accounts.agent_account;
        agent.status = AgentStatus::Deactivated;
        agent.updated_at = Clock::get()?.unix_timestamp;

        msg!("AgentX: agent {} deactivated", agent.name);
        Ok(())
    }
}


// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[event]
pub struct AgentRegisteredEvent {
    pub owner: Pubkey,
    pub agent_id: [u8; 8],
    pub name: String,
    pub timestamp: i64,
}

#[event]
pub struct TaskExecutedEvent {
    pub agent: Pubkey,
    pub task_nonce: u64,
    pub success: bool,
    pub reward_lamports: u64,
    pub timestamp: i64,
}

#[event]
pub struct RewardClaimedEvent {
    pub owner: Pubkey,
    pub amount: u64,
}


// ---------------------------------------------------------------------------
// Custom errors
// ---------------------------------------------------------------------------

#[error_code]
pub enum AgentXError {
    #[msg("Agent name exceeds maximum length")]
    NameTooLong,

    #[msg("Model identifier exceeds maximum length")]
    ModelTooLong,

    #[msg("Task result summary exceeds maximum length")]
    ResultTooLong,

    #[msg("Agent is not in Active status")]
    AgentNotActive,

    #[msg("Agent has been permanently deactivated")]
    AgentDeactivated,

    #[msg("Caller is not authorized for this action")]
    Unauthorized,

    #[msg("New agent registrations are currently paused")]
    RegistrationPaused,

    #[msg("No rewards available to claim")]
    NoRewardsToClaim,
}
