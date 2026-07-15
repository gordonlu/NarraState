use narrastate_case::{adapt_v01, load_case_package};
use narrastate_core::{
    CaseDefinition, CaseManifest, CharacterDefinition, CharacterId, EntityRef, SolutionVariant,
    VariantId,
};
use std::fs;
use std::path::PathBuf;

fn main() {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cases/rain-gallery-variants"));
    let legacy: CaseDefinition =
        serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json"))
            .expect("legacy fixture");
    let mut template =
        adapt_v01(legacy, "1.0.0", VariantId::from("variant-luo")).expect("adapt legacy case");
    template.id = "rain-gallery-variants".into();
    template.title = "雨夜画廊：多重真相".into();
    template.solution_variants[0].id = "variant-luo".into();
    template.solution_variants[0].title = "失控的债务".into();
    template.solution_variants[0].description = "罗成为偿还债务盗走画作。".into();
    template.solution_variants[0].ending.epilogue = "门禁、纤维与典当联系锁定了罗成。".into();
    template.default_variant_id = "variant-luo".into();

    let culprit = template
        .shared_characters
        .iter()
        .find(|character| character.id == CharacterId::from("luo-cheng"))
        .cloned()
        .expect("Luo character");
    let shen = make_variant(
        &template,
        &culprit,
        "variant-shen",
        "shen-an",
        "延误背后的交易",
        "沈岸利用布展延误转移画作，并试图出售。",
    );
    let lin = make_variant(
        &template,
        &culprit,
        "variant-lin",
        "lin-yue",
        "修复师的替换计划",
        "林岳借修复工作接近画作并完成调包。",
    );
    template.solution_variants.extend([shen, lin]);

    fs::create_dir_all(&output).expect("create output directory");
    let manifest = CaseManifest {
        id: template.id.clone(),
        version: template.version.clone(),
        schema_version: template.schema_version.clone(),
        title: template.title.clone(),
        language: template.locale.clone(),
        default_variant_id: template.default_variant_id.clone(),
        variant_count: template.solution_variants.len() as u32,
        generated: false,
        entry: "case.json".into(),
        assets: vec![],
        visual_assets: vec![],
    };
    fs::write(
        output.join("case.json"),
        serde_json::to_vec_pretty(&template).expect("serialize template"),
    )
    .expect("write case.json");
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write manifest.json");
    let package = load_case_package(&output).expect("generated package must validate");
    println!(
        "generated {} variants at {} ({})",
        package.manifest.variant_count,
        output.display(),
        package.template_content_hash
    );
}

fn make_variant(
    template: &narrastate_core::CaseTemplate,
    culprit: &CharacterDefinition,
    variant_id: &str,
    responsible_id: &str,
    title: &str,
    description: &str,
) -> SolutionVariant {
    let mut variant = template.solution_variants[0].clone();
    variant.id = variant_id.into();
    variant.title = title.into();
    variant.description = description.into();
    variant.responsible_character_id = responsible_id.into();
    variant.ending.epilogue = format!("完整证据链最终指向了{title}。");

    let mut former_culprit = culprit.clone();
    former_culprit.claims.clear();
    former_culprit.defenses.clear();
    former_culprit.disclosure_graph.nodes.clear();
    variant
        .character_replacements
        .insert(former_culprit.id.clone(), former_culprit);

    let mut responsible = template
        .shared_characters
        .iter()
        .find(|character| character.id == CharacterId::from(responsible_id))
        .cloned()
        .expect("target character");
    responsible.goals = culprit.goals.clone();
    responsible.knowledge = culprit.knowledge.clone();
    responsible.claims = culprit.claims.clone();
    for claim in &mut responsible.claims {
        claim.owner = responsible.id.clone();
    }
    responsible.defenses = culprit.defenses.clone();
    responsible.disclosure_graph = culprit.disclosure_graph.clone();
    responsible.resilience = culprit.resilience;
    variant
        .character_replacements
        .insert(responsible.id.clone(), responsible);

    for fact in &template.shared_facts {
        if fact.tags.contains("motive") || fact.tags.contains("action") {
            let mut replacement = fact.clone();
            replacement.subject = EntityRef::from(responsible_id);
            variant
                .fact_replacements
                .insert(replacement.id.clone(), replacement);
        }
    }
    for evidence in &template.shared_evidence {
        if evidence
            .elements
            .iter()
            .any(|element| template.required_case_elements.contains(element))
        {
            let mut replacement = evidence.clone();
            replacement.description = format!(
                "{}；该变体的鉴定结果指向 {}。",
                replacement.description, responsible_id
            );
            variant
                .evidence_replacements
                .insert(replacement.id.clone(), replacement);
        }
    }
    variant
}
