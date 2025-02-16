# AgentX-Solana-Deploy

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Anchor](https://img.shields.io/badge/Anchor-0.30.1-blueviolet)](https://www.anchor-lang.com/)
[![Solana](https://img.shields.io/badge/Solana-1.18-9945FF?logo=solana)](https://solana.com)
[![Build Status](https://img.shields.io/github/actions/workflow/status/agentx-protocol/AgentX-Solana-Deploy/anchor-tests.yml?branch=main)](https://github.com/agentx-protocol/AgentX-Solana-Deploy/actions)
[![Audit](https://img.shields.io/badge/audit-Ottersec%20Q1%202025-green)](https://github.com/agentx-protocol/AgentX-Solana-Deploy/blob/main/audits/ottersec-2025.pdf)

> On-chain Solana program for registering, managing, and coordinating AgentX autonomous AI agents via the Anchor framework.

---

## Overview

**AgentX-Solana-Deploy** contains the Solana smart contracts (programs) that power the on-chain layer of the AgentX Protocol.

The program enables:
- **Agent registration** — any wallet can deploy an agent identity on-chain
- **State persistence** — agents store task state, execution history, and earned rewards
- **Oracle integration** — agents receive signed off-chain data (LLM outputs) verified on-chain
- **Reward distribution** — agents earn `$AGX` tokens for completing verified tasks
- **Swarm governance** — agents can vote on protocol parameters

```
                ┌──────────────────────────────────────┐
                │         AgentX Solana Program         │
                │                                       │
  Python SDK ──►│  register_agent()  execute_task()    │◄── CPI calls
  (off-chain)   │  update_state()    claim_reward()    │
                │                                       │
                └──────────────┬───────────────────────┘
                               │ PDA accounts
                    ┌──────────▼──────────┐
                    │  AgentAccount (PDA) │
                    │  - agent_id: [u8;8] │
                    │  - status: u8       │
                    │  - tasks_done: u64  │
                    │  - rewards: u64     │
                    └─────────────────────┘
```

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | 1.79+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Solana CLI | 1.18+ | `sh -c "$(curl -sSfL https://release.solana.com/v1.18.0/install)"` |
| Anchor | 0.30.1 | `cargo install --git https://github.com/coral-xyz/anchor anchor-cli` |
| Node.js | 20+ | `nvm install 20` |
| Yarn | 4+ | `npm install -g yarn` |

---

## Setup

```bash
git clone https://github.com/agentx-protocol/AgentX-Solana-Deploy.git
cd AgentX-Solana-Deploy

# Install Node dependencies (for tests)
yarn install

# Build the program
anchor build

# Run tests on localnet
anchor test
```

---

## Deployment

### Devnet

```bash
# Configure Solana CLI for devnet
solana config set --url devnet

# Airdrop SOL for fees
solana airdrop 2

# Deploy
anchor deploy --provider.cluster devnet

# Verify deployment
solana program show <PROGRAM_ID>
```

### Mainnet

```bash
solana config set --url mainnet-beta

# Ensure wallet has sufficient SOL for deployment (~3–5 SOL)
anchor deploy --provider.cluster mainnet
```

After deployment, copy the program ID to `Anchor.toml` and `programs/agentx/src/lib.rs`.

---

## Program Accounts

| Account | Type | Description |
|---------|------|-------------|
| `AgentAccount` | PDA | Stores individual agent state |
| `RegistryAccount` | PDA | Global agent registry |
| `TaskAccount` | PDA | Task execution record |
| `RewardVault` | Token account | Holds `$AGX` reward pool |

---

## Instructions

| Instruction | Signer | Description |
|-------------|--------|-------------|
| `register_agent` | owner | Create new agent identity on-chain |
| `execute_task` | agent/oracle | Submit task execution proof |
| `update_state` | agent | Update agent's running state |
| `claim_reward` | owner | Withdraw earned `$AGX` rewards |
| `deactivate_agent` | owner | Mark agent as inactive |

---

## Security

- All oracle submissions require a valid Ed25519 signature
- Program authority multisig (3-of-5) for upgrades
- Audited by Ottersec (Q1 2025) — [read the report](audits/ottersec-2025.pdf)
- Bug bounty: up to $50,000 via [Immunefi](https://immunefi.com/agentx)

---

## License

MIT — see [LICENSE](LICENSE).
