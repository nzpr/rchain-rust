(* Rchain core syntax (Phase 0 skeleton) — mirrors spec/Rchain/Syntax.lean. *)

Require Import ZArith String.

(* Ground scalar values. *)
Inductive Ground : Type :=
  | GBool (b : bool)
  | GInt  (n : Z)
  | GStr  (s : string).

(* Variables as de Bruijn levels (bound/free) or a wildcard. *)
Inductive Var : Type :=
  | Bound (level : nat)
  | Free  (level : nat)
  | Wildcard.

(* A process: the flattened Par ADT. `PPar` is the commutative parallel composition |.

   Phase 0 simplification: send/receive/match are binary (single datum/bind/case),
   matching spec/Rchain/Syntax.lean. *)
Inductive Proc : Type :=
  | PNil
  | PGround (g : Ground)
  | PVar   (v : Var)
  | PSend     (chan datum : Proc)
  | PReceive  (bind body : Proc)
  | PNew      (n : nat) (body : Proc)
  | PMatch    (target pat body : Proc)
  | PPar      (p q : Proc).
