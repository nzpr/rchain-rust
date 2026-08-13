# RChain invariant inventory

The catalog of mathematical invariants the Rust rewrite must preserve. Each row links a law to its
source-of-truth location, its Lean formalization, and its Rust test gate.

**Status legend**

- **stated** — theorem statement exists in Lean (Phase 0), proof is a later obligation
- **Phase N** — to be formalized and proven in phase N (1 = execution core, 2 = RSpace, 3 = Rosette,
  4 = Casper, 5 = crypto/storage)
- **axiom** — postulated (cryptographic primitive), not proven, by design
- **open** — recorded question, not yet formalizable

## Laws

| # | Layer | Law | Source of truth | Lean | Status |
|---|-------|-----|-----------------|------|--------|
| 1 | Rholang | `Par`/`ESet`/`EMap` are commutative; canonicalization to a total order is **idempotent** (`sort(sort p) = sort p`); `sort(p\|q) = sort(q\|p)` | `models/src/main/scala/coop/rchain/models/rholang/sorter/ScoreTree.scala`, `ordering.scala`, `SortedParHashSet.scala` | `Rchain/Sort.lean` (`sort_idempotent`, `sort_par_comm`) | **stated** |
| 2 | Rholang | **α/name equivalence** = par order + `\| Nil` + top-level arithmetic + α + added eval/quote | `rholang/src/main/k/rholang/name-equivalence.k:1-14`, `rholang/reference_doc/normalization_process/README.md` | `Rchain/Sort.lean` | Phase 1 |
| 3 | Rholang | **Capture-avoiding de Bruijn substitution**; `sort(subst t) = subst(sort t)` | `rholang/src/main/scala/coop/rchain/rholang/interpreter/Substitute.scala`, `Env.scala` | `Rchain/Subst.lean` | Phase 1 |
| 4 | Rholang | **Reduction (comm)**; first-match-wins; `new` yields fresh unforgeable names | `rholang/.../interpreter/Reduce.scala`, `rholang/src/main/k/rholang/*.k` | `Rchain/Reduce.lean` | Phase 1 |
| 5 | Rholang | **Spatial matching**; a free var is bound at most once (`addedVars.distinct`) | `rholang/.../interpreter/matcher/SpatialMatcher.scala`, `ParCount.scala` | `Rchain/Match.lean` | Phase 1 |
| 6 | Rholang | **No globally free variables** in a program | `rholang/src/main/k/rholang/{free,program-restrictions}.k`, `models/.../HasLocallyFree.scala` | `Rchain/FreeVars.lean` | Phase 1 |
| 7 | RSpace | **Join commutativity** (channel keys hashed in sorted order) | `rspace/.../hashing/StableHashProvider.scala:18-22` | `Rchain/RSpace/Join.lean` | Phase 2 |
| 8 | RSpace | **Deterministic COMM** (produce refs sorted; content-addressed events) | `rspace/.../trace/Event.scala:35-39` | `Rchain/RSpace/Comm.lean` | Phase 2 |
| 9 | RSpace | **Merge is a monoid**; non-conflicting logs commute | `rspace/.../merger/{StateChange,ChannelChange,EventLogMergingLogic}.scala` | `Rchain/RSpace/Merge.lean` | Phase 2 |
| 10 | RSpace | **Merkle determinism**: content-addressed radix trie, collision-free, empty-root | `rspace/.../history/RadixTree.scala:50-68` | `Rchain/RSpace/Merkle.lean` | Phase 2 |
| 11 | RSpace | **Replay determinism**: recomputed COMM ⊆ recorded trace | `rspace/.../ReplayRSpace.scala:68-71` | `Rchain/RSpace/Comm.lean` | Phase 2 |
| 12 | Rosette | **Actor atomicity** (single-threaded `mbox.nextMsg`) | `rosette/README:27-35`, `roscala/.../ob/Actor.scala:52-61` | `Rchain/Rosette/Actor.lean` | Phase 3 |
| 13 | Rosette | **Reflection**: everything is an `Ob`; meta/parent chain; **fork-join barrier** | `roscala/.../ob/{Ob,Meta,Ctxt}.scala`, `Vm.scala:185-188` | `Rchain/Rosette/Ob.lean` | Phase 3 |
| 14 | Casper | **Finality requires > 2/3** bonded stake; fringe = one message per bonded validator (antichain) | `sdk/.../consensus/Stake.scala:8`, `block-storage/.../dag/Finalizer.scala:76,133` | `Rchain/Casper/{Stake,Fringe}.lean` | Phase 4 |
| 15 | Casper | Fringe **monotone by height**; **seen-set monotone** (no regression) | `block-storage/.../dag/MessageMapSyntax.scala:33`, `casper/.../Validate.scala:285` | `Rchain/Casper/Fringe.lean` | Phase 4 |
| 16 | Casper | **Block number** = max(parent)+1; **seqNum** strictly +1; **content addressing** (`hash = Blake2b256(block−{hash,sig})`); **bonds cache = PoS state** | `casper/.../Validate.scala:177,249,256,360`, `ProtoUtil.scala:70` | `Rchain/Casper/Validate.lean` | Phase 4 |
| 17 | Casper | **Merge determinism** (unique min-cost rejection); numeric channels non-negative/no-overflow; RNG merge commutative | `sdk/.../merging/ConflictResolutionLogic.scala:200`, `rholang/.../merging/RholangMergingLogic.scala:90` | `Rchain/Casper/Validate.lean` | Phase 4 |
| 18 | Storage | **Height map contiguous** (no holes); **fringe identity order-independent** | `block-storage/.../BlockMetadataStore.scala:156`, `models/.../FringeData.scala:32` | `Rchain/Casper/Validate.lean` | Phase 5 |
| 19 | Crypto | Blake2b256 canonical hash; `Blake2b512Random` **associative splittable merge**; sig verify/sign; Curve25519 round-trip | `crypto/.../` (`Blake2b512Random`, `Secp256k1`, `Curve25519`) | `Rchain/Crypto/{Random,Spec}.lean` | Phase 5 (**axiom** for primitives) |

## Open questions

1. `casper/src/main/resources/casper.tla` models only the genesis **bootstrap ceremony handshake**,
   not the finality rule. The formal finality spec must be reconstructed from
   `Finalizer.scala` + `MessageMapSyntax.scala`.
2. The `faultTolerance` field asserted in `integration-tests/test/test_dag_correctness.py:105-111`
   is not computed anywhere in this tree — its exact formula is to be recovered or declared an open
   question.
3. `Rosette` (`rosette/`, `roscala/`) is not wired into `build.sbt`; it is treated as **in scope**
   (per decision) but its exact runtime role should be confirmed before Phase 3.
