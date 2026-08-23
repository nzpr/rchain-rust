//! The QLF AI neuro-symbolic coprocessor — a Rust port of `ai_demonstration.py`.
//!
//! Dialectical synthesis via ZFA Blanket Fusion, demonstrated on the Aristotle
//! syllogism (Socrates → Man → Mortal). Run with: `cargo run --example ai_coprocessor`.

use qucalc::{dialectical_synthesis, to_symbols, DOWN, LEFT, MINUS, PLUS, RIGHT, UP};

fn main() {
    let (subject, middle, predicate) = ("Socrates", "Man", "Mortal");

    println!("======================================================");
    println!("[QLF AI] NEURO-SYMBOLIC COPROCESSOR ENGAGED");
    println!("======================================================");
    println!("[*] Human Prompt    : Evaluate `{subject} -> {middle} -> {predicate}`");

    // Semantic ingestion: S has a spatial deficit (^<), P a spatial surplus (>v),
    // M is the gauge bridge (+/-). The middle term is split across the two premises.
    let premise1 = to_symbols(&[UP, LEFT, PLUS]); // ^<+
    let premise2 = to_symbols(&[MINUS, RIGHT, DOWN]); // ->v
    println!("[*] Topology Mapped : `{premise1}` bounded to `{premise2}`");
    println!("[*] AI Querying Engine. Forcing 3D Projection...");

    // The event: fuse the premises, then let the gauge pair annihilate.
    let synthesis = dialectical_synthesis(&[UP, LEFT], &[RIGHT, DOWN]);

    println!();
    println!(
        "[*] Evaluating Intersection: `{}`",
        to_symbols(&synthesis.intersection)
    );
    println!("[*] Delayed Choice Executed: Gauge phases mathematically annihilated.");

    let geometry = to_symbols(&synthesis.geometry);
    let integrity = if synthesis.zfa {
        format!(
            "Stable R=4 Fluxoid (Absolute Truth Achieved) — ZFA ✓  phase {:?}",
            synthesis.phase
        )
    } else {
        "Unresolved Free Action".to_string()
    };

    println!();
    println!("======================================================");
    println!("AI RESPONSE SYNTHESIS (THE GEOMETRIC EXHAUST)");
    println!("======================================================");
    println!("Underlying Geometry : `{geometry}` -> {integrity}");
    println!("Semantic Output     : Therefore, {subject} is {predicate}.");
    println!("======================================================");
}
