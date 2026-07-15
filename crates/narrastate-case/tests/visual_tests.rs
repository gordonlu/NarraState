use narrastate_case::{
    default_visual_specs, generate_optional_visuals, generate_optional_visuals_with_limit,
    install_inline_package_with_visuals, load_case_package, GeneratedVisualOutput,
    VisualGenerationSpec,
};
use narrastate_core::{
    AssetSemanticRole, GeneratedVisualType, VisualAssetId, VisualAssetManifestEntry,
};
use narrastate_runtime::mock::MockImageGenerationProvider;
use narrastate_runtime::ports::{
    GeneratedImageAsset, ImageGenerationProvider, ImageGenerationRequest, ProviderError,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn spec() -> VisualGenerationSpec {
    VisualGenerationSpec {
        id: VisualAssetId::from("cover"),
        visual_type: GeneratedVisualType::CaseCover,
        public_prompt: "不包含线索的港口氛围插画".into(),
        alt_text: "场景示意图".into(),
        width: 1024,
        height: 1024,
    }
}

#[tokio::test]
async fn missing_or_failed_image_provider_never_fails_case_generation() {
    let missing = generate_optional_visuals(None, &[spec()]).await;
    assert!(missing.outputs.is_empty());
    assert_eq!(missing.warnings, ["VISUAL_PROVIDER_NOT_CONFIGURED"]);
    let failed = MockImageGenerationProvider::new(vec![Err(ProviderError::Timeout)]);
    let report = generate_optional_visuals(Some(&failed), &[spec()]).await;
    assert!(report.outputs.is_empty());
    assert!(report.warnings[0].starts_with("VISUAL_PROVIDER_FAILED"));
}

#[tokio::test]
async fn generated_visual_is_always_decorative_and_shared() {
    let provider = MockImageGenerationProvider::new(vec![Ok(GeneratedImageAsset {
        mime_type: "image/png".into(),
        bytes: vec![1, 2, 3],
    })]);
    let report = generate_optional_visuals(Some(&provider), &[spec()]).await;
    assert_eq!(report.outputs.len(), 1);
    assert!(report.outputs[0].manifest.shared_across_variants);
    assert_eq!(report.outputs[0].manifest.path, "assets/visuals/cover.png");
}

#[test]
fn default_specs_cover_every_supported_public_visual_category() {
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/rain-gallery-variants");
    let package = load_case_package(source).unwrap();
    let specs = default_visual_specs(&package.template, "现代画廊");
    for expected in [
        GeneratedVisualType::CaseCover,
        GeneratedVisualType::ChapterIllustration,
        GeneratedVisualType::SceneBackground,
        GeneratedVisualType::LocationAtmosphere,
        GeneratedVisualType::CharacterPortrait,
        GeneratedVisualType::TransitionIllustration,
        GeneratedVisualType::EndingIllustration,
    ] {
        assert!(specs.iter().any(|spec| spec.visual_type == expected));
    }
    assert!(specs
        .iter()
        .all(|spec| !spec.public_prompt.contains("variant-")));
}

struct TrackingImageProvider {
    active: AtomicUsize,
    peak: AtomicUsize,
}

#[async_trait::async_trait]
impl ImageGenerationProvider for TrackingImageProvider {
    async fn generate_image(
        &self,
        _request: &ImageGenerationRequest,
    ) -> Result<GeneratedImageAsset, ProviderError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(GeneratedImageAsset {
            mime_type: "image/png".into(),
            bytes: vec![1],
        })
    }
}

#[tokio::test]
async fn visual_generation_obeys_concurrency_limit_and_preserves_spec_order() {
    let provider = TrackingImageProvider {
        active: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    };
    let specs = (0..6)
        .map(|index| VisualGenerationSpec {
            id: VisualAssetId::from(format!("visual-{index}")),
            ..spec()
        })
        .collect::<Vec<_>>();
    let report = generate_optional_visuals_with_limit(Some(&provider), &specs, 2).await;
    assert_eq!(provider.peak.load(Ordering::SeqCst), 2);
    assert_eq!(report.outputs.len(), specs.len());
    assert!(report
        .outputs
        .iter()
        .zip(&specs)
        .all(|(output, spec)| output.manifest.id == spec.id));
}

#[test]
fn generated_visuals_are_written_and_verified_inside_atomic_package() {
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/rain-gallery-variants");
    let package = load_case_package(source).unwrap();
    let bytes = vec![1, 2, 3, 4];
    let entry = VisualAssetManifestEntry {
        id: VisualAssetId::from("cover"),
        path: "assets/visuals/cover.png".into(),
        content_hash: narrastate_case::raw_content_hash(&bytes),
        visual_type: GeneratedVisualType::CaseCover,
        semantic_role: AssetSemanticRole::Decorative,
        alt_text: "场景示意图".into(),
        shared_across_variants: true,
    };
    let mut manifest = package.manifest;
    manifest.visual_assets = vec![entry.clone()];
    let root = std::env::temp_dir().join(format!(
        "narrastate-visual-install-{}",
        uuid::Uuid::new_v4()
    ));
    let installed = install_inline_package_with_visuals(
        &manifest,
        &package.template,
        None,
        &[GeneratedVisualOutput {
            manifest: entry,
            bytes: bytes.clone(),
        }],
        &root,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(installed.root.join("assets/visuals/cover.png")).unwrap(),
        bytes
    );
    assert_eq!(
        serde_json::to_value(&installed.validation).unwrap(),
        serde_json::to_value(&package.validation).unwrap(),
        "adding decorative images must not change validation or simulation results"
    );
    std::fs::remove_dir_all(root).unwrap();
}
