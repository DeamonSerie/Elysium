# Aura + GRANDPA Consensus (NPoS/session foundation, M1)

## Model
- **Aura** provides slot-based block authorship/import validity (author, timing/slot checks via runtime/node inherent providers).
- **GRANDPA** provides justification verification and finality voting over blocks accepted by execution/import.
- Native Substrate mechanisms are used; there is no custom on-chain re-verification of Aura seals or GRANDPA signatures beyond runtime APIs and client protocol logic.

## Authority management (staking-backed sessions, M1 foundation)
- Authorities and session keys are managed through `pallet_session` (runtime `Session`) with `SessionManager = pallet_session::historical::NoteHistoricalRoot<Runtime, Staking>`.
- `SessionKeys` include `aura` and `grandpa` keys (`runtime/elysium-runtime/src/lib.rs`, `opaque::SessionKeys`); `SessionHandler` uses session key type providers and Aura/Grandpa remain the consensus engines.
- Validators are staking-backed: `ValidatorIdOf` is `pallet_staking::StashOf`, `Session` uses `Staking` for queued/active validator sets and historical exposures, and Aura `DisabledValidators` is wired to `Session`.
- PoA no-op session manager removed. Initial validator selection/rotation plumbing now flows through `pallet-staking` + `pallet-election-provider-multi-phase` (EPM) + `pallet-bags-list` on ECC (`pallet_balances::Instance1`), with on-chain sequential Phragmén fallback/genesis election provider configured for dev/solochain.
- `pallet_authorship` reports block authors to staking for era points; `pallet_offences` and `pallet_session::historical` are wired so GRANDPA equivocation reporting can be processed (`Grandpa` `EquivocationReportSystem` uses offences + historical key ownership proofs). Reward/slash imbalance routing and full M2 economics remain follow-up tuning.
- `AuraApi::authorities`/`GrandpaApi::grandpa_authorities` expose authorities from consensus pallet storage as updated by session/staking integration.

## Genesis / keys
- Chain specs (`node/src/chain_spec.rs`) populate `session.keys` and `staking.stakers`/staking params for initial validators (stash/controller model as per pinned staking API; dev uses self-stash/controller accounts derived from seeds).
- Direct `aura.authorities` / `grandpa.authorities` genesis config is not used; session/staking genesis initializes authority sets and exposures.
- Development uses `//Alice` validator; local testnet uses `//Alice` and `//Bob` validators with Aura/Grandpa session keys and ECC staking bonds/endowments.

## Node integration
- `node/src/service.rs` keeps canonical wiring:
  - Aura import queue with slot/timestamp inherent providers and Grandpa block/justification import.
  - Aura authoring task for authorities.
  - Grandpa voter, justification generation/import period, warp sync, and notification protocol.
  - Offchain workers remain enabled for authority nodes (`sc_offchain`), which supports EPM offchain solution/miner paths where applicable in pinned APIs; no custom election miner service added yet.
- Authority set tracking for finality follows runtime Grandpa/session/staking APIs across era/session changes in devnet configurations.

## ZK validity gate interaction (MVP scaffold)
- `pallet-zk-verifier` stores VK/version/strict mode and verifies block proofs bound to public inputs (`elysium_primitives::ZkPublicInputsV1`).
- `submit_block_proof` accepts root or unsigned submissions (MVP-ASSUMED devnet path) so proof data can be carried in-block.
- Runtime `Core::execute_block` runs `Executive::execute_block` and then applies a mode-aware ZK enforcement hook (`zk_hook`): strict mode requires a valid proof, dev/mock mode may accept missing proofs per pallet policy.
- Post-state root binding inside execution is scaffolded/best-effort (proof-committed post root checked for consistency with submitted inputs); production requires a proving/import pipeline that binds to header commitments/finality as designed.
- **Operational safety:** enabling strict mode without a working proof submission+verification path will reject blocks.

## Limitations / assumptions (M1 foundation)
- NPoS runtime foundation is wired (staking, session historical, bags-list, EPM election provider, authorship, offences, GRANDPA equivocation reporting hooks), but M2 economics/tuning are incomplete: rewards/slash imbalance sinks and payout routing are placeholders, election signed-phase/miner parameters are conservative dev defaults, and no extensive slashing/economics validation yet.
- Sudo retained for dev/staging governance; governance removal/VK governance/PoA→NPoS migration hardening deferred per roadmap.
- ZK verifier includes mock/placeholder verification paths for devnet; not a production soundness guarantee. Proof carriage/import/finality hardening and real zkVM guest/prover integration are follow-up work.
