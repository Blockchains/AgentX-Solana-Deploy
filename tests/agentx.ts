/**
 * AgentX Protocol — Anchor integration tests
 * Run with: anchor test
 */

import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import { assert, expect } from "chai";
import { Agentx } from "../target/types/agentx";

describe("AgentX Protocol", () => {
  // Use localnet configured in Anchor.toml
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Agentx as Program<Agentx>;
  const owner = provider.wallet as anchor.Wallet;

  // Shared test state
  let registryPda: PublicKey;
  let registryBump: number;
  let agentPda: PublicKey;
  const agentId = Buffer.from("test1234");  // 8 bytes

  // -------------------------------------------------------------------------
  // Helper: derive PDAs
  // -------------------------------------------------------------------------
  async function deriveAgentPda(
    ownerKey: PublicKey,
    id: Buffer
  ): Promise<[PublicKey, number]> {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("agent"), ownerKey.toBuffer(), id],
      program.programId
    );
  }

  async function deriveRegistryPda(): Promise<[PublicKey, number]> {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("registry")],
      program.programId
    );
  }

  // -------------------------------------------------------------------------
  // Setup
  // -------------------------------------------------------------------------
  before(async () => {
    [registryPda, registryBump] = await deriveRegistryPda();
    [agentPda] = await deriveAgentPda(owner.publicKey, agentId);
    console.log("  Registry PDA:", registryPda.toBase58());
    console.log("  Agent PDA:   ", agentPda.toBase58());
  });

  // -------------------------------------------------------------------------
  // Test: Register agent
  // -------------------------------------------------------------------------
  describe("register_agent", () => {
    it("creates an AgentAccount PDA with correct initial state", async () => {
      const agentIdArray = Array.from(agentId);

      await program.methods
        .registerAgent(agentIdArray, "test-agent-01", "gpt-4o")
        .accounts({
          agentAccount: agentPda,
          registry: registryPda,
          owner: owner.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const account = await program.account.agentAccount.fetch(agentPda);
      assert.equal(account.name, "test-agent-01");
      assert.equal(account.model, "gpt-4o");
      assert.deepEqual(account.owner, owner.publicKey);
      assert.equal(account.tasksCompleted.toNumber(), 0);
      assert.equal(account.totalRewards.toNumber(), 0);
      // Status 0 = Inactive
      assert.deepEqual(account.status, { inactive: {} });
    });

    it("rejects duplicate registration with same agent_id", async () => {
      const agentIdArray = Array.from(agentId);
      try {
        await program.methods
          .registerAgent(agentIdArray, "duplicate-agent", "gpt-4o")
          .accounts({
            agentAccount: agentPda,
            registry: registryPda,
            owner: owner.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .rpc();
        assert.fail("Expected transaction to fail");
      } catch (err: any) {
        expect(err.message).to.include("already in use");
      }
    });

    it("rejects name longer than 64 characters", async () => {
      const newId = Buffer.from("newid123");
      const [newPda] = await deriveAgentPda(owner.publicKey, newId);
      const longName = "a".repeat(65);

      try {
        await program.methods
          .registerAgent(Array.from(newId), longName, "gpt-4o")
          .accounts({
            agentAccount: newPda,
            registry: registryPda,
            owner: owner.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .rpc();
        assert.fail("Expected NameTooLong error");
      } catch (err: any) {
        expect(err.error.errorCode.code).to.equal("NameTooLong");
      }
    });
  });

  // -------------------------------------------------------------------------
  // Test: Update state
  // -------------------------------------------------------------------------
  describe("update_state", () => {
    it("activates the agent (status -> Active)", async () => {
      await program.methods
        .updateState({ active: {} })
        .accounts({
          agentAccount: agentPda,
          owner: owner.publicKey,
        })
        .rpc();

      const account = await program.account.agentAccount.fetch(agentPda);
      assert.deepEqual(account.status, { active: {} });
    });

    it("rejects update from non-owner", async () => {
      const attacker = Keypair.generate();
      try {
        await program.methods
          .updateState({ paused: {} })
          .accounts({
            agentAccount: agentPda,
            owner: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();
        assert.fail("Expected Unauthorized error");
      } catch (err: any) {
        expect(err.error.errorCode.code).to.equal("Unauthorized");
      }
    });
  });

  // -------------------------------------------------------------------------
  // Test: Execute task
  // -------------------------------------------------------------------------
  describe("execute_task", () => {
    it("records a successful task and credits rewards", async () => {
      const taskNonce = new BN(1);
      const resultHash = new Array(32).fill(0xab);
      const summary = "Completed price analysis for SOL/USDC pair.";
      const rewardLamports = new BN(1_000_000);

      const [taskPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("task"),
          agentPda.toBuffer(),
          taskNonce.toArrayLike(Buffer, "le", 8),
        ],
        program.programId
      );

      await program.methods
        .executeTask(taskNonce, resultHash, summary, rewardLamports, true)
        .accounts({
          taskAccount: taskPda,
          agentAccount: agentPda,
          registry: registryPda,
          oracle: owner.publicKey,   // owner == authority in test env
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const agentAccount = await program.account.agentAccount.fetch(agentPda);
      assert.equal(agentAccount.tasksCompleted.toNumber(), 1);
      assert.equal(agentAccount.totalRewards.toNumber(), 1_000_000);

      const taskAccount = await program.account.taskAccount.fetch(taskPda);
      assert.equal(taskAccount.verified, true);
      assert.equal(taskAccount.resultSummary, summary);
    });

    it("increments tasks_failed for unsuccessful task", async () => {
      const taskNonce = new BN(2);
      const [taskPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("task"),
          agentPda.toBuffer(),
          taskNonce.toArrayLike(Buffer, "le", 8),
        ],
        program.programId
      );

      await program.methods
        .executeTask(taskNonce, new Array(32).fill(0), "Task timed out.", new BN(0), false)
        .accounts({
          taskAccount: taskPda,
          agentAccount: agentPda,
          registry: registryPda,
          oracle: owner.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const account = await program.account.agentAccount.fetch(agentPda);
      assert.equal(account.tasksFailed.toNumber(), 1);
    });
  });

  // -------------------------------------------------------------------------
  // Test: Deactivate agent
  // -------------------------------------------------------------------------
  describe("deactivate_agent", () => {
    it("permanently deactivates the agent", async () => {
      await program.methods
        .deactivateAgent()
        .accounts({
          agentAccount: agentPda,
          owner: owner.publicKey,
        })
        .rpc();

      const account = await program.account.agentAccount.fetch(agentPda);
      assert.deepEqual(account.status, { deactivated: {} });
    });

    it("rejects state update after deactivation", async () => {
      try {
        await program.methods
          .updateState({ active: {} })
          .accounts({ agentAccount: agentPda, owner: owner.publicKey })
          .rpc();
        assert.fail("Expected AgentDeactivated error");
      } catch (err: any) {
        expect(err.error.errorCode.code).to.equal("AgentDeactivated");
      }
    });
  });
});
