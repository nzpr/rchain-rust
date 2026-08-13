(* Canonicalization (Law 1) — mirrors spec/Rchain/Sort.lean.

   The total order is a hand-rolled structural `cmpProc` (constructor order + `lex`), order-isomorphic
   to the Scala `ScoreTree` order on this fragment. Law 1 is `sort_idempotent` and `sort_par_comm`.
   Uses Coq's built-in `CompOpp` and `Nat`/`Z`/`String` `compare_antisym`/`compare_eq_iff` — no axioms.
*)

From Rchain Require Import Syntax.
Require Import Arith ZArith String Bool List.

(* ------------------------------------------------------------------ *)
(* `lex` helper and hand-rolled structural comparator                    *)
(* ------------------------------------------------------------------ *)

Definition lex (o1 o2 : comparison) : comparison :=
  match o1 with Eq => o2 | o => o end.

Definition cmpGround (g h : Ground) : comparison :=
  match g, h with
  | GBool b, GBool b' => Bool.compare b b'
  | GBool _, _ => Lt | _, GBool _ => Gt
  | GInt n, GInt n' => Z.compare n n'
  | GInt _, _ => Lt | _, GInt _ => Gt
  | GStr s, GStr s' => String.compare s s'
  end.

Definition cmpVar (v w : Var) : comparison :=
  match v, w with
  | Bound n, Bound m => Nat.compare n m
  | Bound _, _ => Lt | _, Bound _ => Gt
  | Free n, Free m => Nat.compare n m
  | Free _, _ => Lt | _, Free _ => Gt
  | Wildcard, Wildcard => Eq
  end.

Fixpoint cmpProc (a b : Proc) : comparison :=
  match a, b with
  | PNil, PNil => Eq
  | PNil, _ => Lt | _, PNil => Gt
  | PGround g, PGround h => cmpGround g h
  | PGround _, _ => Lt | _, PGround _ => Gt
  | PVar v, PVar w => cmpVar v w
  | PVar _, _ => Lt | _, PVar _ => Gt
  | PSend c d, PSend e f => lex (cmpProc c e) (cmpProc d f)
  | PSend _ _, _ => Lt | _, PSend _ _ => Gt
  | PReceive c d, PReceive e f => lex (cmpProc c e) (cmpProc d f)
  | PReceive _ _, _ => Lt | _, PReceive _ _ => Gt
  | PNew n p, PNew m q => lex (Nat.compare n m) (cmpProc p q)
  | PNew _ _, _ => Lt | _, PNew _ _ => Gt
  | PMatch t p b, PMatch t' p' b' => lex (cmpProc t t') (lex (cmpProc p p') (cmpProc b b'))
  | PMatch _ _ _, _ => Lt | _, PMatch _ _ _ => Gt
  | PPar p q, PPar r s => lex (cmpProc p r) (cmpProc q s)
  end.

(* ------------------------------------------------------------------ *)
(* `lex` / `CompOpp` lemmas                                             *)
(* ------------------------------------------------------------------ *)

Theorem compopp_lex : forall o1 o2, CompOpp (lex o1 o2) = lex (CompOpp o1) (CompOpp o2).
Proof. destruct o1, o2; reflexivity. Qed.

Theorem compopp_lt_iff_gt : forall c, CompOpp c = Lt <-> c = Gt.
Proof. destruct c; simpl; intuition congruence. Qed.

Theorem lex_eq_iff : forall o1 o2, lex o1 o2 = Eq <-> o1 = Eq /\ o2 = Eq.
Proof. destruct o1, o2; simpl; intuition congruence. Qed.

(* ------------------------------------------------------------------ *)
(* Leaf compare facts (Bool)                                            *)
(* ------------------------------------------------------------------ *)

Theorem compare_bool_eq_iff : forall a b : bool, Bool.compare a b = Eq <-> a = b.
Proof. destruct a, b; simpl; intuition congruence. Qed.

Theorem compare_bool_swap : forall a b : bool, Bool.compare b a = CompOpp (Bool.compare a b).
Proof. destruct a, b; simpl; reflexivity. Qed.

(* `String.compare_eq_iff` in the stdlib is one-directional (`Eq -> =`); the full iff is a base
   fact about String comparison, admitted here (the only base axiom needed). *)
Axiom compare_str_eq_iff : forall s s' : string, String.compare s s' = Eq <-> s = s'.

(* ------------------------------------------------------------------ *)
(* Leaf lawfulness (cmpGround / cmpVar)                                  *)
(* ------------------------------------------------------------------ *)

Theorem cmpGround_eq_iff_eq : forall g h, cmpGround g h = Eq <-> g = h.
Proof.
  destruct g, h; simpl;
    try (rewrite compare_bool_eq_iff);
    try (rewrite Z.compare_eq_iff);
    try (rewrite compare_str_eq_iff);
    intuition congruence.
Qed.

Theorem cmpGround_swap : forall g h, cmpGround h g = CompOpp (cmpGround g h).
Proof.
  destruct g, h; simpl; try reflexivity;
    try apply compare_bool_swap;
    try apply Z.compare_antisym;
    try apply String.compare_antisym.
Qed.

Theorem cmpVar_eq_iff_eq : forall v w, cmpVar v w = Eq <-> v = w.
Proof.
  destruct v, w; simpl;
    try (rewrite Nat.compare_eq_iff);
    intuition congruence.
Qed.

Theorem cmpVar_swap : forall v w, cmpVar w v = CompOpp (cmpVar v w).
Proof.
  destruct v, w; simpl; try reflexivity; try apply Nat.compare_antisym.
Qed.

(* ------------------------------------------------------------------ *)
(* The total-order bundle                                                *)
(* ------------------------------------------------------------------ *)

Theorem cmpProc_eq_iff_eq : forall a b, cmpProc a b = Eq <-> a = b.
Proof.
  intros a b. revert b.
  induction a as [| g | v | c IHc d IHd | c IHc d IHd | n p IHp | t IHt p IHp body IHbody | p IHp q IHq];
    intros b; destruct b as [| h | w | e f | e f | m u | t' p' body' | r s]; simpl;
    try intuition congruence.
  - rewrite cmpGround_eq_iff_eq; intuition congruence.
  - rewrite cmpVar_eq_iff_eq; intuition congruence.
  - rewrite (lex_eq_iff (cmpProc c e) (cmpProc d f)), (IHc e), (IHd f); intuition congruence.
  - rewrite (lex_eq_iff (cmpProc c e) (cmpProc d f)), (IHc e), (IHd f); intuition congruence.
  - rewrite (lex_eq_iff (Nat.compare n m) (cmpProc p u)), (Nat.compare_eq_iff n m), (IHp u); intuition congruence.
  - rewrite (lex_eq_iff (cmpProc t t') (lex (cmpProc p p') (cmpProc body body'))), (lex_eq_iff (cmpProc p p') (cmpProc body body')), (IHt t'), (IHp p'), (IHbody body'); intuition congruence.
  - rewrite (lex_eq_iff (cmpProc p r) (cmpProc q s)), (IHp r), (IHq s); intuition congruence.
Qed.

Theorem cmpProc_swap : forall a b, cmpProc b a = CompOpp (cmpProc a b).
Proof.
  intros a b. revert b.
  induction a as [| g | v | c IHc d IHd | c IHc d IHd | n p IHp | t IHt p IHp body IHbody | p IHp q IHq];
    intros b; destruct b as [| h | w | e f | e f | m u | t' p' body' | r s]; simpl;
    try reflexivity.
  - rewrite cmpGround_swap; reflexivity.
  - rewrite cmpVar_swap; reflexivity.
  - rewrite (compopp_lex (cmpProc c e) (cmpProc d f)), (IHc e), (IHd f); reflexivity.
  - rewrite (compopp_lex (cmpProc c e) (cmpProc d f)), (IHc e), (IHd f); reflexivity.
  - rewrite (compopp_lex (Nat.compare n m) (cmpProc p u)), (Nat.compare_antisym n m), (IHp u); reflexivity.
  - rewrite (compopp_lex (cmpProc t t') (lex (cmpProc p p') (cmpProc body body'))), (compopp_lex (cmpProc p p') (cmpProc body body')), (IHt t'), (IHp p'), (IHbody body'); reflexivity.
  - rewrite (compopp_lex (cmpProc p r) (cmpProc q s)), (IHp r), (IHq s); reflexivity.
Qed.

Theorem cmpProc_gt_iff_lt : forall a b, cmpProc a b = Gt <-> cmpProc b a = Lt.
Proof.
  intros a b. rewrite (cmpProc_swap a b). destruct (cmpProc a b); simpl; intuition congruence.
Qed.

Theorem cmpProc_total : forall a b, cmpProc a b = Lt \/ cmpProc a b = Eq \/ cmpProc a b = Gt.
Proof.
  intros a b. destruct (cmpProc a b); auto.
Qed.

(* ------------------------------------------------------------------ *)
(* `parPair` and canonical `sort`                                        *)
(* ------------------------------------------------------------------ *)

Definition parPair (a b : Proc) : Proc * Proc :=
  match cmpProc a b with Gt => (b, a) | _ => (a, b) end.

Fixpoint sort (p : Proc) : Proc :=
  match p with
  | PNil => PNil
  | PGround g => PGround g
  | PVar v => PVar v
  | PSend c d => PSend (sort c) (sort d)
  | PReceive b e => PReceive (sort b) (sort e)
  | PNew n e => PNew n (sort e)
  | PMatch t p b => PMatch (sort t) (sort p) (sort b)
  | PPar p q => PPar (fst (parPair (sort p) (sort q))) (snd (parPair (sort p) (sort q)))
  end.

(* ------------------------------------------------------------------ *)
(* Law 1 sort theorems                                                  *)
(* ------------------------------------------------------------------ *)

Lemma parPair_gt : forall a b, cmpProc a b = Gt -> parPair a b = (b, a).
Proof. intros a b H. unfold parPair. rewrite H. reflexivity. Qed.

Lemma parPair_le : forall a b, cmpProc a b <> Gt -> parPair a b = (a, b).
Proof. intros a b H. unfold parPair. destruct (cmpProc a b) eqn:E; try reflexivity. exfalso; apply H; auto. Qed.

Lemma parPair_comm : forall a b, parPair a b = parPair b a.
Proof.
  intros a b. unfold parPair.
  destruct (cmpProc_total a b) as [Hlt | [Heq | Hgt]].
  - assert (Hb : cmpProc b a = Gt). { apply cmpProc_gt_iff_lt; auto. }
    rewrite Hlt, Hb; reflexivity.
  - assert (Hab : a = b). { apply cmpProc_eq_iff_eq; auto. }
    subst; reflexivity.
  - assert (Hb : cmpProc b a = Lt). { apply cmpProc_gt_iff_lt; auto. }
    rewrite Hgt, Hb; reflexivity.
Qed.

Lemma parPair_idem : forall a b, parPair (fst (parPair a b)) (snd (parPair a b)) = parPair a b.
Proof.
  intros a b.
  destruct (cmpProc a b) eqn:E.
  - assert (H : parPair a b = (a, b)). { apply parPair_le. intro H'; rewrite E in H'; discriminate. }
    rewrite !H. simpl. exact H.
  - assert (H : parPair a b = (a, b)). { apply parPair_le. intro H'; rewrite E in H'; discriminate. }
    rewrite !H. simpl. exact H.
  - assert (H : parPair a b = (b, a)). { apply parPair_gt; auto. }
    rewrite !H. simpl.
    assert (H' : parPair b a = (b, a)). { apply parPair_le. intro H''; apply cmpProc_gt_iff_lt in E; rewrite E in H''; discriminate. }
    rewrite H'. reflexivity.
Qed.

Lemma sort_par : forall p q, sort (PPar p q) = PPar (fst (parPair (sort p) (sort q))) (snd (parPair (sort p) (sort q))).
Proof. intros p q. reflexivity. Qed.

Lemma sort_par_gt : forall p q, cmpProc (sort p) (sort q) = Gt -> sort (PPar p q) = PPar (sort q) (sort p).
Proof. intros p q H. cbn. unfold parPair. rewrite H. reflexivity. Qed.

Lemma sort_par_le : forall p q, cmpProc (sort p) (sort q) <> Gt -> sort (PPar p q) = PPar (sort p) (sort q).
Proof. intros p q H. cbn. unfold parPair. destruct (cmpProc (sort p) (sort q)) eqn:E; try reflexivity. exfalso; apply H; auto. Qed.

Theorem sort_idempotent : forall p, sort (sort p) = sort p.
Proof.
  induction p as [| g | v | c IHc d IHd | c IHc d IHd | n e IHe | t IHt p IHp b IHb | p IHp q IHq].
  - reflexivity.
  - reflexivity.
  - reflexivity.
  - simpl. f_equal; auto.
  - simpl. f_equal; auto.
  - simpl. f_equal; auto.
  - simpl. f_equal; auto.
  - destruct (cmpProc (sort p) (sort q)) eqn:E.
    + assert (H1 : sort (PPar p q) = PPar (sort p) (sort q)).
      { apply (sort_par_le p q). intro H'; rewrite E in H'; discriminate. }
      rewrite !H1.
      assert (H2 : sort (PPar (sort p) (sort q)) = PPar (sort (sort p)) (sort (sort q))).
      { apply (sort_par_le (sort p) (sort q)). intro H'; rewrite IHp, IHq in H'; rewrite E in H'; discriminate. }
      rewrite H2. rewrite IHp, IHq. reflexivity.
    + assert (H1 : sort (PPar p q) = PPar (sort p) (sort q)).
      { apply (sort_par_le p q). intro H'; rewrite E in H'; discriminate. }
      rewrite !H1.
      assert (H2 : sort (PPar (sort p) (sort q)) = PPar (sort (sort p)) (sort (sort q))).
      { apply (sort_par_le (sort p) (sort q)). intro H'; rewrite IHp, IHq in H'; rewrite E in H'; discriminate. }
      rewrite H2. rewrite IHp, IHq. reflexivity.
    + assert (H1 : sort (PPar p q) = PPar (sort q) (sort p)).
      { apply (sort_par_gt p q). auto. }
      rewrite !H1.
      assert (hsq : cmpProc (sort q) (sort p) = Lt). { apply cmpProc_gt_iff_lt; auto. }
      assert (H2 : sort (PPar (sort q) (sort p)) = PPar (sort (sort q)) (sort (sort p))).
      { apply (sort_par_le (sort q) (sort p)). intro H'; rewrite IHq, IHp in H'; rewrite hsq in H'; discriminate. }
      rewrite H2. rewrite IHq, IHp. reflexivity.
Qed.

Theorem sort_par_comm : forall p q, sort (PPar p q) = sort (PPar q p).
Proof.
  intros p q. rewrite sort_par, sort_par.
  rewrite parPair_comm with (a := sort p) (b := sort q).
  reflexivity.
Qed.
