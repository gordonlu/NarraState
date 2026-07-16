use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::process;
use std::sync::Arc;

use narrastate_runtime::ports::{InstalledCaseRecord, Repository};

use narrastate_case::{
    compile, freeze_case, install_inline_package, load_case_package, migrate_v01_package,
    run_generation_pipeline, select_variant, PackageError, VariantCandidate,
};
use narrastate_core::case::CaseDefinition;
use narrastate_core::character::CharacterRuntimeState;
use narrastate_core::evidence::{
    DiscoveryRule, EvidenceDefinition, EvidenceUsageKind, EvidenceUse,
};
use narrastate_core::id::{CaseId, EvidenceId, SessionId, TurnId, VariantId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone, TransitionTuning};
use narrastate_core::{
    CaseManifest, CaseTemplate, GeneratedCaseDraft, GenerationLimits, GenerationRequest,
    NarrativeEvent, NarrativeEventKind, NarrativeEventPayload, Seed, SessionMode, SessionState,
    SessionStatus, VariantSelection,
};
use narrastate_provider::case_generation::OpenAiCompatibleCaseGenerationProvider;
use narrastate_provider::openai_compatible::OpenAiProvider;
use narrastate_runtime::mock::{MockInterpreter, MockRenderer};
use narrastate_runtime::ports::LlmConfig;
use narrastate_runtime::{DialoguePlanner, TransitionEngine};
use narrastate_server::api::{router, AppState};
use narrastate_storage::SqliteRepository;
use tower_http::services::{ServeDir, ServeFile};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: narrastate-server <command> [args...]");
        eprintln!("Commands:");
        eprintln!("  validate <path>        Validate a case.json file");
        eprintln!("  case <command> [...]   v0.2 package authoring tools");
        eprintln!("  generate-schema [path] Generate the case JSON schema");
        eprintln!("  generate-template-schema [path] Generate the v0.2 template JSON schema");
        eprintln!("  generate-manifest-schema [path] Generate the v0.2 manifest JSON schema");
        eprintln!("  generate-generation-schemas [dir] Generate request and draft JSON schemas");
        eprintln!("  play --case <path>     Interactive interrogation (mock)");
        eprintln!(
            "  serve [--port <port>] [--db <path>] [--cases <dir>] [--web <dir>]  HTTP server"
        );
        process::exit(1);
    }

    match args[1].as_str() {
        "validate" | "validate-case" => cmd_validate(&args[2..]),
        "case" => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(cmd_case(&args[2..]));
        }
        "game" => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(cmd_game(&args[2..]));
        }
        "generate-schema" => cmd_generate_schema(&args[2..]),
        "generate-template-schema" => cmd_generate_template_schema(&args[2..]),
        "generate-manifest-schema" => cmd_generate_manifest_schema(&args[2..]),
        "generate-generation-schemas" => cmd_generate_generation_schemas(&args[2..]),
        "play" => cmd_play(&args[2..]),
        "serve" | "server" => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(cmd_serve(&args[2..]));
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            process::exit(1);
        }
    }
}

async fn cmd_case(args: &[String]) {
    let Some(command) = args.first().map(String::as_str) else {
        eprintln!("Usage: narrastate-server case <validate|simulate|inspect|compile|migrate>");
        process::exit(1);
    };
    match command {
        "generate" => {
            let request_path = required_arg(args, 1, "case generate <request.json> --output <dir>");
            let output = option_value(args, "--output").unwrap_or("cases/generated");
            let request: GenerationRequest =
                serde_json::from_slice(&fs::read(request_path).unwrap_or_else(|e| {
                    eprintln!("GENERATION_REQUEST_READ_FAILED at $: {e}");
                    process::exit(1)
                }))
                .unwrap_or_else(|e| {
                    eprintln!("GENERATION_REQUEST_INVALID at $: {e}");
                    process::exit(1)
                });
            let api_key = std::env::var("NARRASTATE_API_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| {
                    eprintln!("GENERATION_PROVIDER_NOT_CONFIGURED at NARRASTATE_API_KEY");
                    process::exit(1)
                });
            let llm = Arc::new(
                OpenAiProvider::new(LlmConfig {
                    base_url: std::env::var("NARRASTATE_BASE_URL")
                        .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
                    model: std::env::var("NARRASTATE_MODEL")
                        .unwrap_or_else(|_| "gpt-4o-mini".into()),
                    api_key,
                    timeout_secs: std::env::var("NARRASTATE_GENERATION_TIMEOUT_SECS")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .filter(|value| (30..=900).contains(value))
                        .unwrap_or(180),
                    structured_output_max_tokens: std::env::var("NARRASTATE_GENERATION_MAX_TOKENS")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .filter(|value| (4_096..=65_536).contains(value))
                        .unwrap_or(65_536),
                    max_retries: LlmConfig::default().max_retries,
                })
                .unwrap_or_else(|e| {
                    eprintln!("GENERATION_PROVIDER_INVALID at $: {e}");
                    process::exit(1)
                }),
            );
            let provider = OpenAiCompatibleCaseGenerationProvider::new(llm);
            let result =
                run_generation_pipeline(&provider, request.clone(), GenerationLimits::default())
                    .await
                    .unwrap_or_else(|e| {
                        eprintln!("{} at $: {}", e.code, e.message);
                        process::exit(1)
                    });
            let manifest = CaseManifest {
                id: result.template.id.clone(),
                version: result.template.version.clone(),
                schema_version: result.template.schema_version.clone(),
                title: result.template.title.clone(),
                language: result.template.locale.clone(),
                default_variant_id: result.template.default_variant_id.clone(),
                variant_count: result.template.solution_variants.len() as u32,
                generated: true,
                entry: "case.json".into(),
                assets: vec![],
                visual_assets: vec![],
            };
            let report = serde_json::json!({"request":request,"attempts":result.drafts.len(),"repairs":result.repairs,"validation":result.validation});
            let installed =
                install_inline_package(&manifest, &result.template, Some(&report), output)
                    .unwrap_or_else(|error| exit_package_error(error));
            print_json(
                &serde_json::json!({"job_id":result.job_id,"status":"completed","result_path":installed.root}),
            );
        }
        "validate" => {
            let root = required_arg(args, 1, "case validate <package-dir>");
            let package = load_or_exit(root);
            if args.iter().any(|argument| argument == "--json") {
                print_json(&serde_json::json!({
                    "case_id": package.manifest.id,
                    "case_version": package.manifest.version,
                    "template_content_hash": package.template_content_hash,
                    "validation": package.validation,
                }));
                return;
            }
            println!(
                "VALID case={} version={} hash={}",
                package.manifest.id, package.manifest.version, package.template_content_hash
            );
            for report in &package.validation.variant_reports {
                let simulation = report.simulation.as_ref().expect("valid package simulated");
                println!(
                    "variant={} valid={} states={} turns={}",
                    report.variant_id, report.valid, simulation.visited_states, simulation.turns
                );
            }
        }
        "simulate" => {
            let root = required_arg(args, 1, "case simulate <package-dir> [--variant id]");
            let package = load_or_exit(root);
            let requested = option_value(args, "--variant");
            let selected: Vec<_> = package
                .validation
                .variant_reports
                .iter()
                .filter(|report| requested.is_none_or(|id| id == report.variant_id.as_ref()))
                .collect();
            if args.iter().any(|argument| argument == "--json") {
                if selected.is_empty() {
                    eprintln!(
                        "CASE_VARIANT_NOT_FOUND at --variant: {}",
                        requested.unwrap_or_default()
                    );
                    process::exit(1);
                }
                print_json(&selected);
                return;
            }
            let mut matched = false;
            for report in &package.validation.variant_reports {
                if requested.is_some_and(|id| id != report.variant_id.as_ref()) {
                    continue;
                }
                matched = true;
                let result = report.simulation.as_ref().expect("valid package simulated");
                println!(
                    "variant={} success={} states={} turns={} evidence={} disclosures={}",
                    report.variant_id,
                    result.success,
                    result.visited_states,
                    result.turns,
                    result.acquired_evidence_ids.len(),
                    result.reached_disclosure_nodes.len()
                );
            }
            if !matched {
                eprintln!(
                    "CASE_VARIANT_NOT_FOUND at --variant: {}",
                    requested.unwrap_or_default()
                );
                process::exit(1);
            }
        }
        "inspect" => {
            let root = required_arg(args, 1, "case inspect <package-dir>");
            let package = load_or_exit(root);
            if args.iter().any(|argument| argument == "--json") {
                print_json(&serde_json::json!({
                    "manifest": package.manifest,
                    "template_content_hash": package.template_content_hash,
                }));
                return;
            }
            println!("{} ({})", package.manifest.title, package.manifest.id);
            println!("version: {}", package.manifest.version);
            println!("schema: {}", package.manifest.schema_version);
            println!("variants: {}", package.manifest.variant_count);
            println!("default: {}", package.manifest.default_variant_id);
            println!("generated: {}", package.manifest.generated);
            println!("content_hash: {}", package.template_content_hash);
        }
        "compile" => {
            let root = required_arg(args, 1, "case compile <package-dir> [--variant id]");
            let package = load_or_exit(root);
            let variant = option_value(args, "--variant")
                .map(VariantId::from)
                .unwrap_or_else(|| package.manifest.default_variant_id.clone());
            let compiled =
                narrastate_case::compile(&package.template, &variant).unwrap_or_else(|report| {
                    for issue in report.errors {
                        eprintln!(
                            "{} at {}: {} related={:?}",
                            issue.code, issue.path, issue.message, issue.related_ids
                        );
                    }
                    process::exit(1);
                });
            if args.iter().any(|argument| argument == "--json") {
                print_json(&compiled);
                return;
            }
            println!(
                "variant={} compiled_content_hash={}",
                compiled.variant_id, compiled.compiled_content_hash
            );
        }
        "migrate" => {
            let source = required_arg(args, 1, "case migrate <old-case.json> --output <dir>");
            let Some(output) = option_value(args, "--output") else {
                eprintln!("PACKAGE_ARGUMENT_MISSING at --output: output directory is required");
                process::exit(1);
            };
            let package = migrate_v01_package(source, output)
                .unwrap_or_else(|error| exit_package_error(error));
            if args.iter().any(|argument| argument == "--json") {
                print_json(&serde_json::json!({
                    "case_id": package.manifest.id,
                    "case_version": package.manifest.version,
                    "output": package.root,
                    "template_content_hash": package.template_content_hash,
                }));
                return;
            }
            println!(
                "MIGRATED case={} version={} output={}",
                package.manifest.id,
                package.manifest.version,
                package.root.display()
            );
        }
        other => {
            eprintln!("Unknown case command: {other}");
            process::exit(1);
        }
    }
}

fn print_json(value: &impl serde::Serialize) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|error| {
            eprintln!("CLI_JSON_SERIALIZATION_FAILED at $: {error}");
            process::exit(1);
        })
    );
}

async fn cmd_game(args: &[String]) {
    if args.first().map(String::as_str) != Some("create") {
        eprintln!("Usage: narrastate-server game create <case-id> [--variant default|random|id] [--seed value] [--db path] [--cases dir] [--json]");
        process::exit(1);
    }
    let case_id = CaseId::from(required_arg(args, 1, "game create <case-id>"));
    let database = option_value(args, "--db").unwrap_or("data/narrastate.db");
    let cases = option_value(args, "--cases").unwrap_or("cases");
    let repo = SqliteRepository::new(database)
        .await
        .unwrap_or_else(|error| {
            eprintln!("GAME_STORAGE_OPEN_FAILED at --db: {error}");
            process::exit(1);
        });
    repo.backfill_legacy_session_instances()
        .await
        .unwrap_or_else(|error| {
            eprintln!("GAME_LEGACY_BACKFILL_FAILED at --db: {error}");
            process::exit(1);
        });
    load_case_sources(&repo, std::path::Path::new(cases))
        .await
        .unwrap_or_else(|error| {
            eprintln!("GAME_CASE_LOAD_FAILED at --cases: {error}");
            process::exit(1);
        });
    let installed = repo.list_installed_cases().await.unwrap_or_else(|error| {
        eprintln!("GAME_CASE_INDEX_FAILED at --db: {error}");
        process::exit(1);
    });
    let requested_version = option_value(args, "--case-version");
    let record = installed
        .iter()
        .filter(|record| record.case_id == case_id)
        .filter(|record| requested_version.is_none_or(|version| record.case_version == version))
        .max_by_key(|record| cli_version_key(&record.case_version))
        .unwrap_or_else(|| {
            eprintln!("GAME_CASE_NOT_INSTALLED at case_id: {case_id}");
            process::exit(1);
        });
    let package =
        load_case_package(&record.source_path).unwrap_or_else(|error| exit_package_error(error));
    let selection = match option_value(args, "--variant").unwrap_or("default") {
        "default" => VariantSelection::Default,
        "random" => VariantSelection::Random,
        id => VariantSelection::Specific(VariantId::from(id)),
    };
    let seed = option_value(args, "--seed")
        .map(|value| {
            value.parse::<u64>().unwrap_or_else(|error| {
                eprintln!("GAME_SEED_INVALID at --seed: {error}");
                process::exit(1);
            })
        })
        .unwrap_or_else(|| {
            let id = uuid::Uuid::new_v4();
            u64::from_be_bytes(id.as_bytes()[..8].try_into().expect("UUID has eight bytes"))
        });
    let candidates: Vec<_> = package
        .template
        .solution_variants
        .iter()
        .filter(|variant| variant.enabled)
        .filter(|variant| {
            package
                .validation
                .variant_reports
                .iter()
                .any(|report| report.variant_id == variant.id && report.valid)
        })
        .map(|variant| VariantCandidate {
            id: variant.id.clone(),
            weight: variant.weight,
        })
        .collect();
    let variant_id = select_variant(
        &package.template.id,
        &package.template.version,
        &package.template.default_variant_id,
        &selection,
        Seed(seed),
        &candidates,
    )
    .unwrap_or_else(|error| {
        eprintln!("GAME_VARIANT_SELECTION_FAILED at --variant: {error}");
        process::exit(1);
    });
    let compiled = compile(&package.template, &variant_id).unwrap_or_else(|report| {
        for issue in report.errors {
            eprintln!(
                "{} at {}: {} related={:?}",
                issue.code, issue.path, issue.message, issue.related_ids
            );
        }
        process::exit(1);
    });
    let instance = freeze_case(compiled, Seed(seed));
    repo.save_case_instance(&instance)
        .await
        .unwrap_or_else(|error| {
            eprintln!("GAME_INSTANCE_SAVE_FAILED at instance: {error}");
            process::exit(1);
        });
    let definition = &instance.compiled_case.definition;
    let active_character = definition
        .characters
        .first()
        .map(|character| character.id.clone());
    let session = SessionState {
        session_id: SessionId::new(),
        case_id: definition.id.clone(),
        instance_id: Some(instance.instance_id),
        mode: SessionMode::Mock,
        status: SessionStatus::Active,
        current_turn: 0,
        active_character,
        discovered_facts: definition
            .initial_player_knowledge
            .fact_ids
            .iter()
            .cloned()
            .collect(),
        discovered_evidence: definition
            .evidence
            .iter()
            .filter(|evidence| {
                evidence
                    .discoverable_by
                    .iter()
                    .any(|rule| matches!(rule, DiscoveryRule::StartingEvidence))
            })
            .map(|evidence| evidence.id.clone())
            .collect(),
        character_states: definition
            .characters
            .iter()
            .map(|character| {
                (
                    character.id.clone(),
                    CharacterRuntimeState::new(character.resilience),
                )
            })
            .collect(),
        conversation: vec![],
        accusations: vec![],
        revision: 0,
    };
    let event = NarrativeEvent {
        event_id: uuid::Uuid::new_v4(),
        session_id: session.session_id,
        turn_id: None,
        sequence: 0,
        event_type: NarrativeEventKind::SessionCreated,
        schema_version: 1,
        payload: NarrativeEventPayload::SessionCreated {
            state: Box::new(session.clone()),
        },
    };
    repo.create_session(&session, &[event])
        .await
        .unwrap_or_else(|error| {
            eprintln!("GAME_SESSION_CREATE_FAILED at session: {error}");
            process::exit(1);
        });
    let response = serde_json::json!({
        "session_id": session.session_id,
        "instance_id": instance.instance_id,
        "case_id": instance.case_id,
        "case_version": instance.case_version,
        "seed": seed,
    });
    if args.iter().any(|argument| argument == "--json") {
        print_json(&response);
    } else {
        println!(
            "session_id={} instance_id={} case={} version={} seed={}",
            session.session_id, instance.instance_id, instance.case_id, instance.case_version, seed
        );
    }
}

fn cli_version_key(version: &str) -> (u64, u64, u64) {
    let mut parts = version.split('.').map(|part| part.parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn required_arg<'a>(args: &'a [String], index: usize, usage: &str) -> &'a str {
    args.get(index).map(String::as_str).unwrap_or_else(|| {
        eprintln!("Usage: narrastate-server {usage}");
        process::exit(1);
    })
}

fn option_value<'a>(args: &'a [String], option: &str) -> Option<&'a str> {
    args.iter()
        .position(|argument| argument == option)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn load_or_exit(root: &str) -> narrastate_case::LoadedCasePackage {
    load_case_package(root).unwrap_or_else(|error| exit_package_error(error))
}

fn exit_package_error(error: PackageError) -> ! {
    eprintln!("{} at {}: {}", error.code, error.path, error.message);
    process::exit(1)
}

fn cmd_generate_schema(args: &[String]) {
    let path = args
        .first()
        .map(String::as_str)
        .unwrap_or("schemas/narrastate-case.schema.json");
    let schema = schemars::schema_for!(CaseDefinition);
    let json = serde_json::to_string_pretty(&schema).unwrap_or_else(|error| {
        eprintln!("Failed to serialize case schema: {error}");
        process::exit(1);
    });
    let path = std::path::Path::new(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| {
            eprintln!("Failed to create {}: {error}", parent.display());
            process::exit(1);
        });
    }
    fs::write(path, format!("{json}\n")).unwrap_or_else(|error| {
        eprintln!("Failed to write {}: {error}", path.display());
        process::exit(1);
    });
    println!("Generated {}", path.display());
}

fn cmd_generate_template_schema(args: &[String]) {
    let path = args
        .first()
        .map(String::as_str)
        .unwrap_or("schemas/narrastate-case-template-v0.2.schema.json");
    write_schema(path, &schemars::schema_for!(CaseTemplate));
}

fn cmd_generate_manifest_schema(args: &[String]) {
    let path = args
        .first()
        .map(String::as_str)
        .unwrap_or("schemas/narrastate-case-manifest-v0.2.schema.json");
    write_schema(path, &schemars::schema_for!(CaseManifest));
}

fn cmd_generate_generation_schemas(args: &[String]) {
    let root = args.first().map(String::as_str).unwrap_or("schemas");
    write_schema(
        &format!("{root}/narrastate-generation-request-v0.2.schema.json"),
        &schemars::schema_for!(GenerationRequest),
    );
    write_schema(
        &format!("{root}/narrastate-generated-draft-v0.2.schema.json"),
        &schemars::schema_for!(GeneratedCaseDraft),
    );
}

fn write_schema(path: &str, schema: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(schema).unwrap_or_else(|error| {
        eprintln!("Failed to serialize case schema: {error}");
        process::exit(1);
    });
    let path = std::path::Path::new(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| {
            eprintln!("Failed to create {}: {error}", parent.display());
            process::exit(1);
        });
    }
    fs::write(path, format!("{json}\n")).unwrap_or_else(|error| {
        eprintln!("Failed to write {}: {error}", path.display());
        process::exit(1);
    });
    println!("Generated {}", path.display());
}

// ── Serve subcommand ──────────────────────────────────────────────────────

async fn cmd_serve(args: &[String]) {
    let host = std::env::var("NARRASTATE_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let mut port = std::env::var("NARRASTATE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3000u16);
    let mut db_path = std::env::var("DATABASE_URL").unwrap_or_else(|_| "narrastate.db".into());
    let mut cases_dir = "cases".to_string();
    let mut web_dir = std::env::var("NARRASTATE_WEB_DIR").unwrap_or_else(|_| "web/dist".into());

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(8080);
            }
            "--db" => {
                i += 1;
                db_path = args
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| "narrastate.db".into());
            }
            "--cases" => {
                i += 1;
                cases_dir = args.get(i).cloned().unwrap_or_else(|| "cases".into());
            }
            "--web" => {
                i += 1;
                web_dir = args.get(i).cloned().unwrap_or_else(|| "web/dist".into());
            }
            _ => {}
        }
        i += 1;
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Open repository
    let repo = match SqliteRepository::new(&db_path).await {
        Ok(r) => {
            tracing::info!("Connected to database: {db_path}");
            r
        }
        Err(e) => {
            eprintln!("Failed to open database: {e}");
            process::exit(1);
        }
    };

    let backfill = repo
        .backfill_legacy_session_instances()
        .await
        .unwrap_or_else(|error| {
            eprintln!("Failed to backfill legacy session instances: {error}");
            process::exit(1);
        });
    if backfill.migrated_sessions > 0 {
        tracing::info!(
            "Backfilled {} legacy session instance(s)",
            backfill.migrated_sessions
        );
        for limitation in backfill.limitations {
            tracing::warn!("{limitation}");
        }
    }
    let interrupted = repo
        .fail_interrupted_generation_jobs()
        .await
        .unwrap_or_else(|error| {
            eprintln!("Failed to mark interrupted generation jobs: {error}");
            process::exit(1);
        });
    if interrupted > 0 {
        tracing::warn!(interrupted, "marked interrupted generation jobs as failed");
    }

    // Load cases from directory
    if let Err(error) = load_case_sources(&repo, std::path::Path::new(&cases_dir)).await {
        eprintln!("Failed to load cases directory '{cases_dir}': {error}");
        process::exit(1);
    }

    let state = Arc::new(AppState::new(Arc::new(repo)));
    let index = std::path::Path::new(&web_dir).join("index.html");
    let static_files = ServeDir::new(&web_dir).fallback(ServeFile::new(index));
    let app = router(state).fallback_service(static_files);

    let addr = format!("{host}:{port}");
    tracing::info!("NarraState server starting on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Failed to bind {addr}: {e}");
            process::exit(1);
        });

    axum::serve(listener, app).await.unwrap_or_else(|e| {
        eprintln!("Server error: {e}");
        process::exit(1);
    });
}

async fn load_case_sources(repo: &dyn Repository, root: &std::path::Path) -> Result<(), String> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        if directory.join("expected.json").is_file() {
            tracing::debug!("Skipping invalid Golden fixture: {}", directory.display());
            continue;
        }
        if directory.join("manifest.json").is_file() {
            let package = load_case_package(&directory).map_err(|error| error.to_string())?;
            let compiled =
                narrastate_case::compile(&package.template, &package.manifest.default_variant_id)
                    .map_err(|report| report.to_string())?;
            repo.save_case(&compiled.definition)
                .await
                .map_err(|error| error.to_string())?;
            let source_path = fs::canonicalize(&directory)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .into_owned();
            repo.install_case(&InstalledCaseRecord {
                case_id: package.manifest.id.clone(),
                case_version: package.manifest.version.clone(),
                source_path,
                schema_version: package.manifest.schema_version.clone(),
                template_content_hash: package.template_content_hash.to_string(),
            })
            .await
            .map_err(|error| error.to_string())?;
            tracing::info!(
                "Loaded package: {} ({})",
                package.manifest.title,
                package.manifest.id
            );
            continue;
        }
        let entries = fs::read_dir(&directory).map_err(|error| error.to_string())?;
        for entry in entries {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|name| name == "case.json") {
                let json = fs::read_to_string(&path).map_err(|error| error.to_string())?;
                let case: CaseDefinition =
                    serde_json::from_str(&json).map_err(|error| error.to_string())?;
                repo.save_case(&case)
                    .await
                    .map_err(|error| error.to_string())?;
                tracing::info!("Loaded legacy case: {} ({})", case.title, case.id);
            }
        }
    }
    Ok(())
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
        let _result = engine.process_with_requirements(
            &action,
            &mut state,
            culprit,
            &evidence_map,
            &case.required_case_elements,
            turn_id,
        );
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

    let resolved_case_path = if std::path::Path::new(&case_path).is_file() {
        std::path::PathBuf::from(&case_path)
    } else {
        std::path::Path::new("cases")
            .join(&case_path)
            .join("case.json")
    };
    let json = fs::read_to_string(&resolved_case_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", resolved_case_path.display(), e);
        process::exit(1);
    });
    let case: CaseDefinition = serde_json::from_str(&json).unwrap_or_else(|e| {
        eprintln!("Error parsing {}: {}", resolved_case_path.display(), e);
        process::exit(1);
    });

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();

    let player_known_facts: BTreeSet<_> = case
        .initial_player_knowledge
        .fact_ids
        .iter()
        .cloned()
        .collect();
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
        if let Some(unknown) = attached_ids
            .iter()
            .find(|id| !evidence_map.contains_key(*id))
        {
            eprintln!("Unknown evidence ID: {unknown}");
            continue;
        }

        let action = interpreter.interpret(&text, &attached_ids);
        let turn_id = TurnId::new();

        let result = engine.process_with_requirements(
            &action,
            &mut state,
            character,
            &evidence_map,
            &case.required_case_elements,
            turn_id,
        );

        let plan = planner.plan_with_context(
            &action,
            &mut state,
            character,
            &evidence_map,
            &player_known_facts,
            result.diff.newly_revealed_disclosures.first(),
        );
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
