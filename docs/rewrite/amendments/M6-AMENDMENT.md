# M6 amendment to the frozen rewrite contract

Ruling: `SCENARIOS.md` rows beat `SPEC.md` prose; the M6 brief's MUST HAVES and invariants outrank
both where no row exists. Three architecture questions were asked; the dialogs were cancelled, and
the reviewer then issued a binding ruling on all three. That ruling is implemented verbatim and
recorded below; nothing in S310-S312 is an unconfirmed assumption any more. New rows start at S310.
Every row below is marked IN SCOPE or REQUIRES WIRING. These rows extend, and do not rewrite, the
M6 rows already in `SCENARIOS.md`.

This module claims M6 rows S100–S106, S134–S137, S145, S156, S171–S177, S185, S186, S190–S220 and
the new S310–S327 below. Rows owned by M4/M5/M7 are not claimed as M6 tests. S196/S206 decide that
control-plane journal and attempt outcomes are permitted in all three initialized states; the M6
"attempt outcomes/checkpoints when authority is auto" sentence in `SPEC.md` is therefore read as
"non-authority domain attempt persistence", and the amendment records that reading.

## Entry-point enumeration and proof that each passes the one gate

`gate::decide` in `crates/energy-decide/src/authority/gate.rs` is the only function that answers a
write-authority question. `Authority::decide` is its only caller and increments one audit counter
per decision; the following table is exhaustive for this crate. Each named path either calls
`Authority::decide` directly or the M4 port implementation that delegates to it.

| Entry point | How it reaches the single gate | Test evidence |
|---|---|---|
| startup / bootstrap resolution | `startup_steps::append_record` calls `gate::decide` for the diagnostic record before any device gate opens; sealed token+event commit is the documented pre-gate exception | S190, S194, S218, S219 |
| mode endpoints and legacy aliases | `authorize_write(ControlPlaneAuthority)`; an `auto` request cannot change a latch | S172, S102 |
| audit expiry / config reload | `authorize_write(ControlPlaneJournal/DomainPersistence)`; refusal records `dry_run_latch_preserved{path}` | S173-style path, S145, S185 |
| API request | `decline_operational_dry_run` and `authorize_write`; no handler-local flag | S102, S196, S205 |
| generic `ws call` passthrough | `authorize_ws_passthrough` for device and settings classes; settings can never establish authority | S185 |
| schedules / watchdog restart / internal recovery | `authorize_persistence`, `authorize_reconcile`; reconcile needs the authority-internal capability | S176/S177-style paths, S206 |
| tick dispatch / M4 port | `impl WriteAuthority for Authority` delegates every command to `Authority::decide` at execution time | S145, S197, S230 |
| maintenance and learning persistence | `authorize_persistence(DomainPersistence)`; runtime-only denial outside auto | S196 |
| clearance command | `begin_clearance`/`commit_clearance`/`fail_clearance` all call the gate before any state change | S195, S178, S183 |
| queued / replayed device intent | `execute_intent` re-checks at execution, never at enqueue; refusal is a typed row | S200 |
| manual persisted-state edit / raw ledger write | detected at startup; `CheckpointWrite` and all authoritative writes are refused by the gate | S186, S209, S216 |
| authority anchor update | storage-only seam under `AnchorStore`; no engine handler writes it outside the Exact ordering | S213–S216, S219 |

## State machine

The only operational authority states are `dry_run_latched → clearing → auto`. `clearing` is
entered first by a logged operator command with actor, instant, clearance id and health check; it is
fenced, bounded and terminal, and commit to `auto` requires journal ordering plus all-pack neutral
evidence (M5 owns the evidence timing). Failure or timeout returns to `dry_run_latched` with a typed
reason. There is no `continuity_proven` state and no other operational state.

`recovery_required` is a startup resolution verdict plus invariant, not an operational state.
Every startup resolves to exactly `reconciled` or `recovery_required`; dispatch never branches on a
fourth value. When the verdict holds, effective authority is `dry_run_latched` (zero device writes)
and every attempted write returns the typed `authority_recovery_required`, even when the phase is
`AUTHORITY_INVALID` (S319, S320). Health and API expose a distinct `recovery_required` condition,
the universal banner plus the failing reason/heads/anchor evidence, and never fold it into a plain
`dry_run_latched` presentation. The verdict is locked in the separate anchor AND the append-only
resolution records: no timeout, mode PATCH, settings write or restart can clear it. Only a successful
authority-only reconcile consumes a `ReconcilePermit`, appends a `reconciled` record, clears the
anchor marker and audits who/why/what evidence (S319, S324).

## Documented limitation carried into code and tests

The separate-file anchor is on the same filesystem as the ledger. It detects database-only
replace/copy/rollback, a missing or mismatched anchor (including a divergent `journal_hash_chain_head`), a
torn/corrupt anchor, a truncated journal and a stale checkpoint. It CANNOT detect a coherent rollback of BOTH ledger and anchor (a full add-on
snapshot restore): there is no TPM, remote attestation, server clock or signing account. Such a
restore is locally indistinguishable from a crash and lands in `reconciled` with the universal banner
and a journalled startup-resolution record. This limitation is documented in
`src/authority/mod.rs`, `src/authority/anchor.rs` and tested by S208/S211/S212/S326; it is not
papered over by an invented continuity claim. The anchor also holds the issued bootstrap marker with
the run and transaction identity and the locked recovery evidence; resolution records are a second
durable copy of the recovery verdict so replacing the anchor cannot clear it.

## New rows

| ID | Module | Title | Given | When | Then | Type | REAL | Scope | Why |
|---|---|---|---|---|---|---|---|---|---|
| S310 | M6 | recovery is a verdict and invariant, not a state | token missing/malformed on a post-token ledger | startup resolves authority | gate states stay exactly `dry_run_latched`, `clearing`, `auto`; effective authority is latched; every write returns `authority_recovery_required`; health/API show a distinct `recovery_required` condition with failing evidence | contract | no | IN SCOPE (binding ruling) | Reviewer separated operational state from the right to write; SPEC prose corrected by this row. |
| S311 | M6 | separate-file anchor is real inside the pure crate | fresh install and later starts in a temp data directory | `Authority::start` runs | anchor is a JSON file atomically replaced with temp write, file fsync, rename and parent-directory fsync; resolution record is a JSONL file; ledger remains a typed port because SQLite lives outside this crate | integration | no | IN SCOPE (binding ruling) | SPEC asks for a literal separate file; the decision dialog was cancelled, so the more testable literal implementation was chosen. |
| S312 | M6 | sealed transaction rejects out-of-scope capabilities | one `BootstrapAuthorityTx` scope | device write, unrelated journal append, settings write or queue work is attempted through it | Rust has no method for those writes; the runtime capability entry returns `bootstrap_tx_capability_denied`; only token, matching event and commit exist and the commit is non-cloneable | contract | no | IN SCOPE (binding ruling) | SPEC allows runtime denial where the language cannot enforce absent capabilities; no compile-fail dependency was added offline. |
| S313 | M6 | one gate is observable for every entry point | each enumerated entry point is exercised once | decisions are read back | gate-call count increases exactly once per decision, every audit row carries actor/entry/class/state/phase/outcome/why, and no handler-local authorization flag exists | contract | no | IN SCOPE | Owner requirement to enumerate and prove the single gate, including generic passthrough. |
| S314 | M6 | coherent dual-file rollback stays a disclosed limitation | ledger and anchor snapshots are restored together | engine restarts | resolution is `reconciled`, banner is shown and a resolution record is appended; no test or doc claims proven continuity, while a database-only rollback is `anchor_mismatch` | regression | no | IN SCOPE | SPEC LIMITATIONS and S208; the boundary of local detection must never be overstated. |
| S315 | M6 | reconcile is authority-only at the real boundary | ordinary API caller asks to reconcile dispatch uncertainty | `authorize_reconcile(&caller)` and `Dispatcher::reconcile` run | ordinary caller receives `reconcile_denied`; `Dispatcher::reconcile` now takes `&ReconcilePermit`, so there is no call path without the capability | contract | no | IN SCOPE | Reviewer required the deferred M4 wiring item now; M4 crate is unwired and the public signature change is allowed. |
| S316 | M6 | safety traffic is identified before the authority gate in clearing | an M5 neutralisation hold is needed in `clearing` | M4 asks authority before M5 | the request must be `SlotRequest::safety` with a fence so the gate sees device-neutralisation class; an economic request in clearing is denied regardless of a later M5 preemption | contract | no | REQUIRES WIRING | M4's `prepare` asks authority before safety, so an economic request cannot retroactively become a clearing-permitted safety write. |
| S317 | M6 | startup-resolution record is writable in every phase | authority uninitialized, invalid or journal incomplete | startup resolution runs | exactly one diagnostic record is appended with case and hashes; it has no authority, head, checkpoint, domain-state or device-command capability | contract | no | IN SCOPE | Later SPEC prose and S217 decide this against the earlier "only token+event" sentence; control-plane and device classes remain closed. |
| S318 | M6 | passthrough authorization is not a service policy | generic `ws call` is authorized in `auto` | gate answers | the gate only answers the write-authority question; HA service/entity allow-listing and target validation stay in the server and are not claimed by M6 | contract | no | REQUIRES WIRING | SPEC says the gate answer derives from M5 state only; this crate has no HA service registry, so wiring must still validate the call. |

## Interpretations that need reviewer attention

* The decision dialogs for the three architecture questions were cancelled twice, then the
  reviewer issued a binding ruling. S310-S312 record that ruling, not unconfirmed defaults.
* `AuthorityLedger::restore_snapshot`, `corrupt_generation`, `remove_latch`, `set_checkpoint` and
  `truncate_tail` are storage seams representing external database edits and restore tooling, not
  engine write paths. They intentionally let fault tests build adversarial ledgers; the engine
  never calls them.
* `CommittedBootstrap::ledger_snapshot` exists only for fault tests; the commit itself consumes the
  transaction and cannot be cloned or replayed.
* `Dispatcher::reconcile` is still public, but its third argument is a `&ReconcilePermit`; the
  two-argument call no longer compiles. API callers obtain a permit only from the authority owner.
* The anchor is checked against the ledger generation, token hash, checkpoint AND journal
  hash-chain head; a divergent chain is `anchor_mismatch`, never a silently accepted history.
* A corrupt anchor file is an anchor-load error: one diagnostic record is still appended, the
  phase is `AUTHORITY_INVALID` and the typed reason is `resolution_record_corrupt`.
* When anchor-ref installation fails AFTER the one diagnostic record was appended, that record
  retains the pre-durability case while the gate resolves `recovery_required` from the typed
  storage failure. This is the residue of "exactly one record per startup" combined with a
  failure after the record; wiring should mirror the final outcome in the ledger audit row.
* `BootstrapAuthorityTx::deny_extra_capability` and `Authority::reject_bootstrap_reconstruction`
  are the runtime fallbacks for capabilities Rust cannot express as absent. The issuance capability
  itself has private fields, no `Clone` and no storage constructor; `begin` is crate-private. The
  typed denial reaches the audit row and API, never a panic or silent no-op (S321-S323).

## Reviewer-ruling rows

| ID | Module | Title | Given | When | Then | Type | REAL | Scope | Why |
|---|---|---|---|---|---|---|---|---|---|
| S319 | M6 | recovery survives restart and only authority reconcile clears it | corrupted anchor produces `recovery_required` | engine restarts twice, then an authority operator calls `issue_reconcile_permit` and `reconcile_recovery` | both restarts stay `recovery_required` and refuse with `authority_recovery_required`; the permitted reconcile appends a `reconciled` record, clears the anchor marker and audits who/why/evidence; the next restart is `reconciled` | regression | no | IN SCOPE | Reviewer finding 87; inverts the old test that expected a restart to clear recovery. |
| S320 | M6 | typed recovery refusal outranks an invalid phase | ledger generation is malformed | any write class asks the gate in the `AUTHORITY_INVALID` phase | reason is always `authority_recovery_required`; the evidence is a separate structured detail with ledger head, anchor head, checkpoint and chain head; health condition is `recovery_required`, not `dry_run_latched` | regression | no | IN SCOPE | Reviewer finding 88 and ruling condition 3. |
| S321 | M6 | durable token evidence cannot rebuild the bootstrap capability | only stored latch generation, hash, anchor and resolution records remain | an API caller attempts a bootstrap reconstruction | `begin` is crate-private; `begin_external` returns `bootstrap_tx_capability_denied` or `bootstrap_replay_denied` as a typed audit row, never a token or a capability | contract | no | IN SCOPE | Reviewer ruling 3 and finding 85; the token is evidence, the capability is single-run. |
| S322 | M6 | previous-run and substituted-transaction markers are refused | anchor carries an unconsumed issue from another run or another tx id | reconstruction is attempted | typed `bootstrap_tx_capability_denied`; no token is minted and no ledger is changed | contract | no | IN SCOPE | Reviewer finding 85; binding to run and transaction identity. |
| S323 | M6 | from-scratch replay with every durable artefact still fails | ledger snapshot, anchor file and all resolution records are available | replay calls `begin_external` | marker is consumed, so `bootstrap_replay_denied` is returned and surfaced through `reject_bootstrap_reconstruction` as `bootstrap_tx_capability_denied` | regression | no | IN SCOPE | Reviewer finding 85; S210 only covered the easy post-commit case. |
| S324 | M6 | reconcile permit is consumed at the dispatcher boundary | dispatcher has unresolved futures and an ordinary caller | reconcile is called with the authority permit; an API caller tries without one | the two-argument call does not compile; the permitted call runs; `authorize_reconcile` denies an ordinary operator with `reconcile_denied` | contract | no | IN SCOPE | Reviewer finding 86; deferred M4 wiring completed in this round. |
| S325 | M6 | named entry points are enforced, not merely counted | latched and auto authorities with the same ledger shape | engine dispatch, safety traffic, boot/shutdown neutralisation, settings/mode control plane and generic websocket passthrough are exercised | each is refused while latched and authorized in auto; no per-handler flag can change the answer | regression | no | IN SCOPE | Reviewer finding 89. |
| S326 | M6 | anchor attack matrix is explicit | anchor ahead of the database, database from another install, database rolled back only, or both rolled back together | each restart resolves startup | the first three are `anchor_mismatch` / `recovery_required`; the coherent both-file rollback is `reconciled` and documented as locally indistinguishable | regression | no | IN SCOPE | Reviewer ruling 2; makes the limitation deliberate rather than silent. |
| S327 | M6 | recovery evidence is part of the API payload | checkpoint is stale | health is read | structured evidence contains the reason and both heads; banner names the failure; API code string contains `authority_recovery_required` | contract | no | IN SCOPE | Reviewer ruling condition 3; distinguishes operator dry_run from unknown authority. |
