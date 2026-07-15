use crate::raw_content_hash;
use futures_util::{stream, StreamExt};
use narrastate_core::{
    AssetSemanticRole, CaseTemplate, GeneratedVisualType, VisualAssetId, VisualAssetManifestEntry,
};
use narrastate_runtime::ports::{ImageGenerationProvider, ImageGenerationRequest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualGenerationSpec {
    pub id: VisualAssetId,
    pub visual_type: GeneratedVisualType,
    pub public_prompt: String,
    pub alt_text: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct GeneratedVisualOutput {
    pub manifest: VisualAssetManifestEntry,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct VisualGenerationReport {
    pub outputs: Vec<GeneratedVisualOutput>,
    pub warnings: Vec<String>,
}

pub const DEFAULT_VISUAL_GENERATION_CONCURRENCY: usize = 3;

/// Builds only variant-neutral prompts from shared/public case material.
pub fn default_visual_specs(template: &CaseTemplate, setting: &str) -> Vec<VisualGenerationSpec> {
    let shared_context = format!("案件《{}》。公开简介：{}", template.title, template.summary);
    let mut specs = vec![
        VisualGenerationSpec {
            id: VisualAssetId::from("case-cover"),
            visual_type: GeneratedVisualType::CaseCover,
            public_prompt: format!(
                "{shared_context}。案件封面氛围插画，不得包含文字、证据、线索、嫌疑暗示或人物有罪暗示。"
            ),
            alt_text: format!("{}视觉氛围图，不作为案件证据", template.title),
            width: 1024,
            height: 1024,
        },
        VisualGenerationSpec {
            id: VisualAssetId::from("scene-setting"),
            visual_type: GeneratedVisualType::SceneBackground,
            public_prompt: format!(
                "{shared_context}。公开场景设定：{setting}。宽幅场景氛围图，不出现具体人物、关键物品、精确建筑结构、出入口、摄像头、密道、文字或可推理细节。"
            ),
            alt_text: "案件场景氛围示意图，不作为案件证据".into(),
            width: 1536,
            height: 1024,
        },
        VisualGenerationSpec {
            id: VisualAssetId::from("chapter-opening"),
            visual_type: GeneratedVisualType::ChapterIllustration,
            public_prompt: format!(
                "{shared_context}。调查章节开场插画，表现即将开始调查的氛围，不出现责任人、证据、线索、文字或隐藏事实。"
            ),
            alt_text: "调查章节开场氛围图，不作为案件证据".into(),
            width: 1536,
            height: 1024,
        },
        VisualGenerationSpec {
            id: VisualAssetId::from("transition-investigation"),
            visual_type: GeneratedVisualType::TransitionIllustration,
            public_prompt: format!(
                "{shared_context}。调查过程的抽象转场插画，不出现人物、证据、线索、文字或真相暗示。"
            ),
            alt_text: "调查转场氛围图，不作为案件证据".into(),
            width: 1536,
            height: 1024,
        },
        VisualGenerationSpec {
            id: VisualAssetId::from("ending-generic"),
            visual_type: GeneratedVisualType::EndingIllustration,
            public_prompt: format!(
                "{shared_context}。案件调查结束后的通用收束氛围插画，不描绘责任人、结案方式、关键物品、证据、文字或具体真相。"
            ),
            alt_text: "案件结束氛围图，不作为案件证据".into(),
            width: 1536,
            height: 1024,
        },
    ];
    specs.extend(
        template
            .entities
            .iter()
            .filter(|entity| entity.kind.eq_ignore_ascii_case("location"))
            .take(6)
            .enumerate()
            .map(|(index, location)| VisualGenerationSpec {
                id: VisualAssetId::from(format!("location-{index}")),
                visual_type: GeneratedVisualType::LocationAtmosphere,
                public_prompt: format!(
                    "{shared_context}。地点：{}。地点氛围示意图，不出现具体人物、关键物品位置、精确出入口结构、摄像头、密道、文字、证据或可用于时间线判断的细节。",
                    location.name
                ),
                alt_text: format!("{}地点氛围图，不作为案件证据", location.name),
                width: 1536,
                height: 1024,
            }),
    );
    specs.extend(template.shared_characters.iter().map(|character| {
        VisualGenerationSpec {
            id: VisualAssetId::from(format!("character-{}", character.id)),
            visual_type: GeneratedVisualType::CharacterPortrait,
            public_prompt: format!(
                "中性角色头像。姓名：{}，公开角色：{}，公开介绍：{}。表情与光线保持中性，不得暗示善恶、有罪、隐藏身份、证据或真相变体。",
                character.name, character.role, character.public_profile
            ),
            alt_text: format!("{}的角色示意头像", character.name),
            width: 512,
            height: 512,
        }
    }));
    specs
}

pub async fn generate_optional_visuals(
    provider: Option<&dyn ImageGenerationProvider>,
    specs: &[VisualGenerationSpec],
) -> VisualGenerationReport {
    generate_optional_visuals_with_limit(provider, specs, DEFAULT_VISUAL_GENERATION_CONCURRENCY)
        .await
}

pub async fn generate_optional_visuals_with_limit(
    provider: Option<&dyn ImageGenerationProvider>,
    specs: &[VisualGenerationSpec],
    max_concurrency: usize,
) -> VisualGenerationReport {
    let Some(provider) = provider else {
        return VisualGenerationReport {
            outputs: vec![],
            warnings: vec!["VISUAL_PROVIDER_NOT_CONFIGURED".into()],
        };
    };
    let mut completed = stream::iter(specs.iter().cloned().enumerate())
        .map(|(index, spec)| async move {
            if spec.public_prompt.trim().is_empty() {
                return (index, spec, Err("VISUAL_PROMPT_EMPTY".to_string()));
            }
            let result = provider
                .generate_image(&ImageGenerationRequest {
                    visual_type: spec.visual_type,
                    prompt: spec.public_prompt.clone(),
                    alt_text: spec.alt_text.clone(),
                    width: spec.width,
                    height: spec.height,
                })
                .await
                .map_err(|error| format!("VISUAL_PROVIDER_FAILED:{error}"));
            (index, spec, result)
        })
        .buffer_unordered(max_concurrency.max(1))
        .collect::<Vec<_>>()
        .await;
    completed.sort_by_key(|(index, _, _)| *index);

    let mut report = VisualGenerationReport::default();
    for (_, spec, result) in completed {
        match result {
            Ok(image) => {
                let extension = match image.mime_type.as_str() {
                    "image/webp" => "webp",
                    "image/jpeg" => "jpg",
                    _ => "png",
                };
                report.outputs.push(GeneratedVisualOutput {
                    manifest: VisualAssetManifestEntry {
                        id: spec.id.clone(),
                        path: format!("assets/visuals/{}.{}", spec.id, extension),
                        content_hash: raw_content_hash(&image.bytes),
                        visual_type: spec.visual_type,
                        semantic_role: AssetSemanticRole::Decorative,
                        alt_text: spec.alt_text,
                        shared_across_variants: true,
                    },
                    bytes: image.bytes,
                });
            }
            Err(error) => report.warnings.push(format!("{error}:{}", spec.id)),
        }
    }
    report
}
