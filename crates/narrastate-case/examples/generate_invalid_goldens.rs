use narrastate_case::load_case_package;
use narrastate_core::{CaseManifest, CaseTemplate, CharacterId, InterrogationPhase, VariantId};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cases/golden-invalid"));
    let valid = load_case_package("cases/rain-gallery-variants").expect("valid Golden Package");

    let mut divergence = valid.template.clone();
    let mut duplicate = divergence.solution_variants[0].clone();
    duplicate.id = VariantId::from("variant-luo-copy");
    duplicate.title = "只换标题的伪变体".into();
    duplicate.ending.epilogue = "只改结局文案。".into();
    divergence.solution_variants.push(duplicate);
    write_case(
        &output,
        "insufficient-divergence",
        divergence,
        &["VARIANT_INSUFFICIENT_DIVERGENCE"],
    );

    let mut deadlock = valid.template.clone();
    let culprit = deadlock
        .shared_characters
        .iter_mut()
        .find(|character| character.id == CharacterId::from("luo-cheng"))
        .expect("culprit");
    for node in &mut culprit.disclosure_graph.nodes {
        node.min_phase = InterrogationPhase::ConfessionEligible;
    }
    write_case(
        &output,
        "disclosure-deadlock",
        deadlock,
        &["DISCLOSURE_NODE_UNREACHABLE", "SIMULATION_TURN_LIMIT"],
    );

    let mut hidden = valid.template.clone();
    for evidence in &mut hidden.shared_evidence {
        if evidence
            .elements
            .iter()
            .any(|element| hidden.required_case_elements.contains(element))
        {
            evidence.discoverable_by.clear();
        }
    }
    write_case(
        &output,
        "hidden-required-evidence",
        hidden,
        &["COMPILE_REQUIRED_ELEMENTS_UNMAPPABLE"],
    );

    let mut false_confession = valid.template.clone();
    let graph = false_confession
        .shared_characters
        .iter()
        .find(|character| character.id == CharacterId::from("luo-cheng"))
        .expect("culprit")
        .disclosure_graph
        .clone();
    false_confession
        .shared_characters
        .iter_mut()
        .find(|character| character.id == CharacterId::from("shen-an"))
        .expect("false suspect")
        .disclosure_graph = graph;
    write_case(
        &output,
        "false-suspect-confession",
        false_confession,
        &["COMPILE_RESPONSIBILITY_GRAPH_MISMATCH"],
    );
}

fn write_case(root: &Path, name: &str, template: CaseTemplate, expected_codes: &[&str]) {
    let directory = root.join(name);
    fs::create_dir_all(&directory).expect("create Golden directory");
    let manifest = CaseManifest {
        id: template.id.clone(),
        version: template.version.clone(),
        schema_version: template.schema_version.clone(),
        title: format!("Invalid Golden: {name}"),
        language: template.locale.clone(),
        default_variant_id: template.default_variant_id.clone(),
        variant_count: template.solution_variants.len() as u32,
        generated: false,
        entry: "case.json".into(),
        assets: vec![],
        visual_assets: vec![],
    };
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("write manifest");
    fs::write(
        directory.join("case.json"),
        serde_json::to_vec_pretty(&template).expect("case JSON"),
    )
    .expect("write case");
    fs::write(
        directory.join("expected.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"codes": expected_codes}))
            .expect("expected JSON"),
    )
    .expect("write expected codes");
}
