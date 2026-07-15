use narrastate_case::{
    adapt_v01, install_inline_package, load_case_package, migrate_v01_package, raw_content_hash,
};
use narrastate_core::{
    AssetManifestEntry, AssetSemanticRole, CaseDefinition, CaseManifest, CaseTemplate, ContentHash,
    GeneratedVisualType, VariantId, VisualAssetId, VisualAssetManifestEntry,
};
use std::fs;
use std::path::{Path, PathBuf};

struct TempPackage(PathBuf);

impl TempPackage {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("narrastate-package-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempPackage {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn template() -> CaseTemplate {
    let legacy: CaseDefinition =
        serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap();
    adapt_v01(legacy, "1.0.0", VariantId::from("classic")).unwrap()
}

fn manifest(template: &CaseTemplate) -> CaseManifest {
    CaseManifest {
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
    }
}

fn write_package(root: &Path, template: &CaseTemplate, manifest: &CaseManifest) {
    fs::write(
        root.join("case.json"),
        serde_json::to_vec_pretty(template).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn valid_package_loads_and_runs_all_validation() {
    let root = TempPackage::new();
    let template = template();
    write_package(root.path(), &template, &manifest(&template));

    let loaded = load_case_package(root.path()).unwrap();
    assert!(loaded.validation.valid);
    assert_eq!(loaded.template.id, template.id);
    assert!(loaded.template_content_hash.as_ref().starts_with("sha256:"));
}

#[test]
fn manifest_cannot_escape_package_root() {
    let root = TempPackage::new();
    let template = template();
    let mut manifest = manifest(&template);
    manifest.entry = "../outside.json".into();
    write_package(root.path(), &template, &manifest);

    let error = load_case_package(root.path()).unwrap_err();
    assert_eq!(error.code, "PACKAGE_PATH_UNSAFE");
    assert_eq!(error.path, "manifest.json.entry");
}

#[test]
fn asset_hash_mismatch_is_explicit() {
    let root = TempPackage::new();
    let template = template();
    fs::create_dir(root.path().join("assets")).unwrap();
    fs::write(root.path().join("assets/cover.webp"), b"actual bytes").unwrap();
    let mut manifest = manifest(&template);
    manifest.assets.push(AssetManifestEntry {
        path: "assets/cover.webp".into(),
        content_hash: ContentHash::from("sha256:deadbeef"),
    });
    write_package(root.path(), &template, &manifest);

    let error = load_case_package(root.path()).unwrap_err();
    assert_eq!(error.code, "PACKAGE_ASSET_HASH_MISMATCH");
    assert_eq!(error.path, "manifest.json.assets[0].content_hash");
}

#[test]
fn valid_asset_hash_participates_in_template_hash() {
    let root = TempPackage::new();
    let template = template();
    fs::create_dir(root.path().join("assets")).unwrap();
    let bytes = b"cover bytes";
    fs::write(root.path().join("assets/cover.webp"), bytes).unwrap();
    let mut manifest = manifest(&template);
    manifest.assets.push(AssetManifestEntry {
        path: "assets/cover.webp".into(),
        content_hash: raw_content_hash(bytes),
    });
    write_package(root.path(), &template, &manifest);

    assert!(load_case_package(root.path()).is_ok());
}

#[test]
fn visual_assets_cannot_be_variant_specific() {
    let root = TempPackage::new();
    let template = template();
    fs::create_dir(root.path().join("assets")).unwrap();
    let bytes = b"portrait";
    fs::write(root.path().join("assets/portrait.webp"), bytes).unwrap();
    let mut manifest = manifest(&template);
    manifest.visual_assets.push(VisualAssetManifestEntry {
        id: VisualAssetId::from("portrait-one"),
        path: "assets/portrait.webp".into(),
        content_hash: raw_content_hash(bytes),
        visual_type: GeneratedVisualType::CharacterPortrait,
        semantic_role: AssetSemanticRole::Decorative,
        alt_text: "角色氛围头像".into(),
        shared_across_variants: false,
    });
    write_package(root.path(), &template, &manifest);
    assert_eq!(
        load_case_package(root.path()).unwrap_err().code,
        "GENERATED_IMAGE_LEAKS_VARIANT"
    );
}

#[test]
fn generated_visual_cannot_be_stored_as_evidence_original() {
    let root = TempPackage::new();
    let template = template();
    fs::create_dir_all(root.path().join("assets/evidence")).unwrap();
    let bytes = b"not evidence";
    fs::write(root.path().join("assets/evidence/fake.webp"), bytes).unwrap();
    let mut manifest = manifest(&template);
    manifest.visual_assets.push(VisualAssetManifestEntry {
        id: VisualAssetId::from("fake-evidence"),
        path: "assets/evidence/fake.webp".into(),
        content_hash: raw_content_hash(bytes),
        visual_type: GeneratedVisualType::SceneBackground,
        semantic_role: AssetSemanticRole::Decorative,
        alt_text: "场景示意图".into(),
        shared_across_variants: true,
    });
    write_package(root.path(), &template, &manifest);
    assert_eq!(
        load_case_package(root.path()).unwrap_err().code,
        "GENERATED_IMAGE_USED_AS_EVIDENCE"
    );
}

#[test]
fn generated_package_requires_generation_report() {
    let root = TempPackage::new();
    let template = template();
    let mut manifest = manifest(&template);
    manifest.generated = true;
    write_package(root.path(), &template, &manifest);

    let error = load_case_package(root.path()).unwrap_err();
    assert_eq!(error.code, "PACKAGE_FILE_MISSING");
    assert_eq!(error.path, "generation-report.json");
}

#[test]
fn restricted_version_rejects_non_numeric_shortcuts() {
    let root = TempPackage::new();
    let template = template();
    let mut manifest = manifest(&template);
    manifest.version = "latest".into();
    write_package(root.path(), &template, &manifest);

    let error = load_case_package(root.path()).unwrap_err();
    assert_eq!(error.code, "PACKAGE_VERSION_INVALID");
}

#[cfg(unix)]
#[test]
fn package_entry_may_not_be_a_symbolic_link() {
    use std::os::unix::fs::symlink;
    let root = TempPackage::new();
    let template = template();
    let manifest = manifest(&template);
    fs::write(
        root.path().join("real-case.json"),
        serde_json::to_vec_pretty(&template).unwrap(),
    )
    .unwrap();
    symlink("real-case.json", root.path().join("case.json")).unwrap();
    fs::write(
        root.path().join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let error = load_case_package(root.path()).unwrap_err();
    assert_eq!(error.code, "PACKAGE_SYMLINK_FORBIDDEN");
}

#[test]
fn legacy_migration_writes_valid_package_atomically() {
    let root = TempPackage::new();
    let source = root.path().join("legacy.json");
    let output = root.path().join("migrated");
    fs::write(
        &source,
        include_bytes!("../../../cases/rain-gallery/case.json"),
    )
    .unwrap();

    let migrated = migrate_v01_package(&source, &output).unwrap();
    assert_eq!(migrated.manifest.variant_count, 1);
    assert_eq!(
        migrated.manifest.default_variant_id,
        VariantId::from("classic")
    );
    assert!(output.join("manifest.json").is_file());
    assert!(output.join("case.json").is_file());

    let error = migrate_v01_package(&source, &output).unwrap_err();
    assert_eq!(error.code, "PACKAGE_OUTPUT_EXISTS");
}

#[test]
fn inline_install_uses_controlled_case_and_version_directory() {
    let root = TempPackage::new();
    let template = template();
    let manifest = manifest(&template);
    let installed = install_inline_package(&manifest, &template, None, root.path()).unwrap();
    assert_eq!(
        installed.root,
        root.path().join("rain-gallery").join("1.0.0")
    );
    let repeated = install_inline_package(&manifest, &template, None, root.path()).unwrap();
    assert_eq!(
        repeated.template_content_hash,
        installed.template_content_hash
    );
}

#[test]
fn inline_install_rejects_path_like_case_id_before_writing() {
    let root = TempPackage::new();
    let template = template();
    let mut manifest = manifest(&template);
    manifest.id = "../escape".into();
    let error = install_inline_package(&manifest, &template, None, root.path()).unwrap_err();
    assert_eq!(error.code, "PACKAGE_PATH_COMPONENT_INVALID");
    assert!(!root.path().join("escape").exists());
}
