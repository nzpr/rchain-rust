(* Canonicalization (Law 1) — mirrors spec/Rchain/Sort.lean and ScoreTree.scala. *)

From Rchain Require Import Syntax.
Require Import Arith ZArith List.

(* Lexicographic comparison of two score trees (list nat). *)
Fixpoint lexNat (l1 l2 : list nat) : comparison :=
  match l1 with
  | nil =>
      match l2 with
      | nil => Eq
      | _ => Lt
      end
  | a :: as =>
      match l2 with
      | nil => Gt
      | b :: bs =>
          match Nat.compare a b with
          | Eq => lexNat as bs
          | o => o
          end
      end
  end.

(* Score of a ground value (string comparison is a Phase 1 refinement). *)
Definition scoreGround (g : Ground) : list nat :=
  match g with
  | GBool b => 1 :: (if b then 1 :: nil else 0 :: nil)
  | GInt n  => 2 :: Z.abs_nat n :: nil
  | GStr _  => 3 :: nil
  end.

(* Score of a variable (de Bruijn level distinguishes bound/free; wildcard constant). *)
Definition scoreVar (v : Var) : list nat :=
  match v with
  | Bound l  => 1 :: l :: nil
  | Free l   => 2 :: l :: nil
  | Wildcard => 3 :: nil
  end.

(* Flattened score tree of a process (constructor constant, then children). *)
Fixpoint score (p : Proc) : list nat :=
  match p with
  | PNil            => 0 :: nil
  | PGround g       => 10 :: scoreGround g
  | PVar v          => 20 :: scoreVar v
  | PSend c d       => 30 :: app (score c) (score d)
  | PReceive b e    => 40 :: app (score b) (score e)
  | PNew n e        => 50 :: n :: score e
  | PMatch t pat b  => 60 :: app (score t) (app (score pat) (score b))
  | PPar p q        => 999 :: app (score p) (score q)
  end.

(* Total order on processes (lexicographic on their score trees). *)
Definition cmpProc (a b : Proc) : comparison := lexNat (score a) (score b).

(* Order two sorted subterms of a par into a canonical (smaller-first) pair. *)
Definition parPair (a b : Proc) : Proc * Proc :=
  match cmpProc a b with
  | Gt => (b, a)
  | _  => (a, b)
  end.

(* Canonicalization: recursively sort every subterm and order par children. *)
Fixpoint sort (p : Proc) : Proc :=
  match p with
  | PNil            => PNil
  | PGround g       => PGround g
  | PVar v          => PVar v
  | PSend c d       => PSend (sort c) (sort d)
  | PReceive b e    => PReceive (sort b) (sort e)
  | PNew n e        => PNew n (sort e)
  | PMatch t pat b  => PMatch (sort t) (sort pat) (sort b)
  | PPar p q        =>
      let (a, b) := parPair (sort p) (sort q) in
      PPar a b
  end.

(* Atomic fixed points. *)
Lemma sort_nil : sort PNil = PNil.
Proof. reflexivity. Qed.

Lemma sort_ground : forall g : Ground, sort (PGround g) = PGround g.
Proof. intros g. reflexivity. Qed.

Lemma sort_var : forall v : Var, sort (PVar v) = PVar v.
Proof. intros v. reflexivity. Qed.

(* Law 1 (idempotence) — Phase 1 proof obligation (admitted). *)
Theorem sort_idempotent : forall p : Proc, sort (sort p) = sort p.
Proof. Admitted.

(* Law 1 (commutativity of par under normalization) — Phase 1 proof obligation (admitted). *)
Theorem sort_par_comm : forall p q : Proc, sort (PPar p q) = sort (PPar q p).
Proof. Admitted.
