use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::process;

use narrastate_core::case::CaseDefinition;
use narrastate_core::character::CharacterRuntimeState;
use narrastate_core::evidence::{EvidenceDefinition, EvidenceUsageKind, EvidenceUse};
use narrastate_core::id::{EvidenceId, TurnId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone, TransitionTuning};
use narrastate_runtime::mock::{MockInterpreter, MockRenderer};
use narrastate_runtime::{DialoguePlanner, TransitionEngine};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: narrastate-server <command> [args...]");
        eprintln!("Commands:");
        eprintln!("  validate <path>    Validate a case.json file");
        eprintln!("  play --case <path> [--mock]  Interactive interrogation");
        process::exit(1);
    }

    match args[1].as_str() {
        "validate" | "validate-case" => cmd_validate(&args[2..]),
        "play" => cmd_play(&args[2..]),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            process::exit(1);
        }
    }
}

fn cmd_validate(args: &[String]) {
    let path = args
        .first()
        .expect("Usage: narrastate-server validate <path>");
    let json = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", path, e);
        process::exit(1);
    });
    let case: CaseDefinition = serde_json::from_str(&json).unwrap_or_else(|e| {
        eprintln!("Error parsing {}: {}", path, e);
        process::exit(1);
    });
    println!("Case: {} ({})", case.title, case.id);
    println!("  {} facts", case.facts.len());
    println!("  {} evidence", case.evidence.len());
    println!("  {} characters", case.characters.len());

    let mut all_ok = true;

    match case.validate() {
        Ok(()) => println!("✅ Structural validation passed"),
        Err(errors) => {
            all_ok = false;
            for e in &errors {
                println!("  ❌ {}", e);
            }
        }
    }

    if !check_reachability(&case) {
        all_ok = false;
    }

    if all_ok {
        println!("✅ Case fully validated, reachability confirmed");
    } else {
        process::exit(1);
    }
}

fn check_reachability(case: &CaseDefinition) -> bool {
    let culprit = case
        .characters
        .iter()
        .find(|c| c.disclosure_graph.confession_node().is_some());

    let Some(culprit) = culprit else {
        println!("❌ No culprit character found");
        return false;
    };
    println!("  Culprit: {} ({})", culprit.name, culprit.id);

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();

    let facts: BTreeSet<_> = case.facts.iter().map(|f| f.id.clone()).collect();
    let culprit_claim_ids: Vec<_> = culprit.claims.iter().map(|c| c.id.clone()).collect();

    let relevant_evidence: Vec<&EvidenceDefinition> = case
        .evidence
        .iter()
        .filter(|e| {
            e.contradicts
                .iter()
                .any(|cid| culprit_claim_ids.contains(cid))
        })
        .collect();

    if relevant_evidence.is_empty() {
        println!("❌ No evidence contradicts the culprit's claims");
        return false;
    }

    println!(
        "  {} evidence items relevant to culprit",
        relevant_evidence.len()
    );

    let engine = TransitionEngine::new(TransitionTuning::default());
    let mut state = CharacterRuntimeState::new(culprit.resilience);

    for (i, ev) in relevant_evidence.iter().enumerate() {
        let usage = EvidenceUse {
            evidence_id: ev.id.clone(),
            usage: EvidenceUsageKind::DirectReference,
        };
        let action = InterpretedAction {
            intent: PlayerIntent::PresentEvidence,
            topics: vec![format!("证据 {}", i + 1)],
            referenced_entities: vec![],
            referenced_claims: vec![],
            evidence_usage: vec![usage],
            asserted_propositions: vec![],
            tone: PlayerTone::Neutral,
            confidence: 1.0,
        };
        let turn_id = TurnId::new();
        let _result = engine.process(&action, &mut state, culprit, &evidence_map, &facts, turn_id);
    }

    println!(
        "  Final phase: {:?}, stress: {}, defense: {}",
        state.phase, state.stress, state.defense_budget
    );
    println!(
        "  Confronted evidence: {}, Disclosures revealed: {}",
        state.confronted_evidence.len(),
        state.revealed_disclosures.len()
    );

    if state.phase >= InterrogationPhase::Cornered {
        println!("  ✅ Reachable — at least Cornered");
        if state.phase >= InterrogationPhase::ConfessionEligible {
            println!("  ✅ ConfessionEligible reachable with all evidence");
        }
        true
    } else {
        println!("  ❌ Unreachable — only reached {:?}", state.phase);
        false
    }
}

// ── Play Command ────────────────────────────────────────────────────────

fn cmd_play(args: &[String]) {
    let mut case_path = String::new();
    let mut _mock_mode = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--case" => {
                i += 1;
                case_path = args.get(i).expect("--case requires a path").clone();
            }
            "--mock" => _mock_mode = true,
            _ => {}
        }
        i += 1;
    }

    if case_path.is_empty() {
        eprintln!("Usage: narrastate-server play --case <path> [--mock]");
        process::exit(1);
    }

    let json = fs::read_to_string(&case_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", case_path, e);
        process::exit(1);
    });
    let case: CaseDefinition = serde_json::from_str(&json).unwrap_or_else(|e| {
        eprintln!("Error parsing {}: {}", case_path, e);
        process::exit(1);
    });

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();

    let facts: BTreeSet<_> = case.facts.iter().map(|f| f.id.clone()).collect();
    let engine = TransitionEngine::new(TransitionTuning::default());
    let planner = DialoguePlanner;
    let interpreter = MockInterpreter;
    let renderer = MockRenderer;

    println!("╔══════════════════════════════════════════╗");
    println!("║  NarraState — CLI Simulator (--mock)    ║");
    println!("║  {}     ║", case.title);
    println!("╚══════════════════════════════════════════╝");
    println!();

    // Let player select a character to interrogate
    println!("Characters:");
    for (ci, ch) in case.characters.iter().enumerate() {
        println!("  {}. {} ({}) — {}", ci + 1, ch.name, ch.id, ch.role);
    }
    print!(
        "Select character to interrogate (1-{}): ",
        case.characters.len()
    );
    io::stdout().flush().ok();

    let stdin = io::stdin();
    let mut input = String::new();
    stdin.read_line(&mut input).ok();
    let char_idx: usize = input.trim().parse::<usize>().unwrap_or(1).saturating_sub(1);
    let char_idx = char_idx.min(case.characters.len().saturating_sub(1));

    let character = &case.characters[char_idx];
    let mut state = CharacterRuntimeState::new(character.resilience);

    println!();
    println!(
        "╔══ Interrogating: {} ({}) ══╗",
        character.name, character.role
    );
    println!("║  Type your question or statement below.");
    println!("║  Attach evidence by adding evidence IDs in brackets:");
    println!("║    \"你为什么在这里？ [ev_card_log]\"");
    println!("║  Commands: /phase, /state, /evidence, /help, /quit");
    println!("╚══════════════════════════════════════════╝");
    println!();

    let mut turn_count = 0u64;

    loop {
        print!("[Turn {}] You: ", turn_count + 1);
        io::stdout().flush().ok();

        input.clear();
        let bytes_read = stdin.read_line(&mut input).ok();
        match bytes_read {
            Some(0) | None => break,
            _ => {}
        }

        let line = input.trim();
        if line.is_empty() {
            continue;
        }

        match line {
            "/quit" | "/exit" => break,
            "/help" => {
                println!("Commands:");
                println!("  /phase      Show current interrogation phase");
                println!("  /state      Show full character state");
                println!("  /evidence   List available evidence");
                println!("  /help       This help");
                println!("  /quit       Exit");
                println!();
                continue;
            }
            "/phase" => {
                println!("  Phase: {:?}", state.phase);
                println!();
                continue;
            }
            "/state" => {
                println!("  Phase:       {:?}", state.phase);
                println!("  Stress:      {}", state.stress);
                println!("  Composure:   {}", state.composure);
                println!("  Trust:       {}", state.trust);
                println!("  Defense:     {}", state.defense_budget);
                println!("  Confronted:  {:?}", state.confronted_evidence);
                println!("  Revealed:    {:?}", state.revealed_disclosures);
                println!();
                continue;
            }
            "/evidence" => {
                println!("Available evidence:");
                for ev in &case.evidence {
                    let known = state.confronted_evidence.contains(&ev.id);
                    println!(
                        "  {} {} — {}",
                        if known { "✓" } else { " " },
                        ev.title,
                        if known { &ev.description } else { "?" }
                    );
                }
                println!();
                continue;
            }
            _ => {}
        }

        // Parse evidence attachments: text [ev_id1] [ev_id2]
        let (text, attached_ids) = parse_evidence_attachments(line);

        let action = interpreter.interpret(&text, &attached_ids);
        let turn_id = TurnId::new();

        let result = engine.process(
            &action,
            &mut state,
            character,
            &evidence_map,
            &facts,
            turn_id,
        );

        let plan = planner.plan(&action, &state, character, &evidence_map);
        let utterance = renderer.render(&plan);

        turn_count += 1;

        // Show state diff
        let d = &result.diff;
        println!();
        println!("  ├─ Phase: {:?} → {:?}", d.phase_before, d.phase_after);
        if d.stress_before != d.stress_after {
            println!(
                "  ├─ Stress: {} → {} ({:+#})",
                d.stress_before,
                d.stress_after,
                d.stress_after as i16 - d.stress_before as i16
            );
        }
        if d.defense_budget_before != d.defense_budget_after {
            println!(
                "  ├─ Defense: {} → {} ({:+#})",
                d.defense_budget_before,
                d.defense_budget_after,
                d.defense_budget_after as i16 - d.defense_budget_before as i16
            );
        }
        if d.composure_before != d.composure_after {
            println!(
                "  ├─ Composure: {} → {} ({:+#})",
                d.composure_before,
                d.composure_after,
                d.composure_after as i16 - d.composure_before as i16
            );
        }
        if d.trust_before != d.trust_after {
            println!(
                "  ├─ Trust: {} → {} ({:+#})",
                d.trust_before,
                d.trust_after,
                d.trust_after as i16 - d.trust_before as i16
            );
        }
        if let Some(reason) = &d.transition_reason {
            println!("  ├─ Reason: {:?}", reason);
        }
        if !d.newly_revealed_disclosures.is_empty() {
            let names: Vec<String> = d
                .newly_revealed_disclosures
                .iter()
                .map(|d| d.to_string())
                .collect();
            println!("  ├─ New disclosures: {}", names.join(", "));
        }
        if !result.contradictory_claims.is_empty() {
            let names: Vec<String> = result
                .contradictory_claims
                .iter()
                .map(|c| c.to_string())
                .collect();
            println!("  ├─ Contradicted claims: {}", names.join(", "));
        }

        println!();
        println!("  {}: {}", character.name, utterance.utterance);
        println!();
    }

    println!("Case concluded after {} turns.", turn_count);
}

fn parse_evidence_attachments(input: &str) -> (String, Vec<EvidenceId>) {
    let mut text = input.to_string();
    let mut ids = Vec::new();

    while let Some(open) = text.find('[') {
        let close = open
            + match text[open..].find(']') {
                Some(p) => p,
                None => break,
            };
        let id_str = text[open + 1..close].trim().to_string();
        text.replace_range(open..=close, "");
        ids.push(EvidenceId::from(id_str));
    }

    (text.trim().to_string(), ids)
}
