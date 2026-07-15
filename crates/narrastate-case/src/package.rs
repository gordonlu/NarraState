use crate::adapt_v01;
use crate::GeneratedVisualOutput;
use crate::{canonical_hash, raw_content_hash, validate_template, CaseValidationReport, HashError};
use narrastate_core::{CaseDefinition, CaseManifest, CaseTemplate, ContentHash, VariantId};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub struct LoadedCasePackage {
    pub root: PathBuf,
    pub manifest: CaseManifest,
    pub template: CaseTemplate,
    pub template_content_hash: ContentHash,
    pub validation: CaseValidationReport,
}

#[derive(Debug, thiserror::Error)]
#[error("{code} at {path}: {message}")]
pub struct PackageError {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

pub fn load_case_package(root: impl AsRef<Path>) -> Result<LoadedCasePackage, PackageError> {
    let root = root.as_ref();
    if !root.is_dir() {
        return Err(package_error(
            "PACKAGE_ROOT_NOT_DIRECTORY",
            "$",
            format!("{} is not a directory", root.display()),
        ));
    }
    reject_symlink(root, "$root")?;
    let manifest_path = safe_file(root, "manifest.json", "manifest.json")?;
    let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
        package_error("PACKAGE_READ_FAILED", "manifest.json", error.to_string())
    })?;
    let manifest: CaseManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        package_error(
            "PACKAGE_MANIFEST_INVALID",
            "manifest.json",
            error.to_string(),
        )
    })?;
    validate_manifest_shape(&manifest)?;

    let entry_path = safe_file(root, &manifest.entry, "manifest.json.entry")?;
    let template_bytes = fs::read(&entry_path).map_err(|error| {
        package_error(
            "PACKAGE_READ_FAILED",
            "manifest.json.entry",
            error.to_string(),
        )
    })?;
    let template: CaseTemplate = serde_json::from_slice(&template_bytes).map_err(|error| {
        package_error(
            "PACKAGE_TEMPLATE_INVALID",
            &manifest.entry,
            error.to_string(),
        )
    })?;
    validate_manifest_identity(&manifest, &template)?;

    let mut asset_hashes = Vec::new();
    for (index, asset) in manifest.assets.iter().enumerate() {
        let path_field = format!("manifest.json.assets[{index}].path");
        let asset_path = safe_file(root, &asset.path, &path_field)?;
        let bytes = fs::read(&asset_path).map_err(|error| {
            package_error("PACKAGE_READ_FAILED", &path_field, error.to_string())
        })?;
        let actual = raw_content_hash(&bytes);
        if actual != asset.content_hash {
            return Err(package_error(
                "PACKAGE_ASSET_HASH_MISMATCH",
                format!("manifest.json.assets[{index}].content_hash"),
                format!("expected {}, calculated {actual}", asset.content_hash),
            ));
        }
        asset_hashes.push((asset.path.as_str(), asset.content_hash.as_ref()));
    }
    for (index, asset) in manifest.visual_assets.iter().enumerate() {
        let field_root = format!("manifest.json.visual_assets[{index}]");
        if !asset.shared_across_variants {
            return Err(package_error(
                "GENERATED_IMAGE_LEAKS_VARIANT",
                format!("{field_root}.shared_across_variants"),
                "decorative visuals must be shared by every truth variant",
            ));
        }
        if asset.path.starts_with("assets/evidence/") {
            return Err(package_error(
                "GENERATED_IMAGE_USED_AS_EVIDENCE",
                format!("{field_root}.path"),
                "generated visuals may not be stored as evidence originals",
            ));
        }
        let path_field = format!("{field_root}.path");
        let asset_path = safe_file(root, &asset.path, &path_field)?;
        let bytes = fs::read(&asset_path).map_err(|error| {
            package_error("PACKAGE_READ_FAILED", &path_field, error.to_string())
        })?;
        let actual = raw_content_hash(&bytes);
        if actual != asset.content_hash {
            return Err(package_error(
                "PACKAGE_ASSET_HASH_MISMATCH",
                format!("{field_root}.content_hash"),
                format!("expected {}, calculated {actual}", asset.content_hash),
            ));
        }
        asset_hashes.push((asset.path.as_str(), asset.content_hash.as_ref()));
    }
    if manifest.generated {
        safe_file(root, "generation-report.json", "generation-report.json")?;
    }

    let template_content_hash =
        canonical_hash(&(&template, &manifest.schema_version, &asset_hashes))
            .map_err(hash_error)?;
    let validation = validate_template(&template);
    if !validation.valid {
        let first = validation
            .errors
            .first()
            .expect("invalid report has errors");
        return Err(package_error(
            "PACKAGE_CASE_INVALID",
            &first.path,
            format!("{}: {}", first.code, first.message),
        ));
    }
    Ok(LoadedCasePackage {
        root: root.to_path_buf(),
        manifest,
        template,
        template_content_hash,
        validation,
    })
}

pub fn migrate_v01_package(
    source: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<LoadedCasePackage, PackageError> {
    let source = source.as_ref();
    let output = output.as_ref();
    if output.exists() {
        return Err(package_error(
            "PACKAGE_OUTPUT_EXISTS",
            "$output",
            format!("{} already exists", output.display()),
        ));
    }
    let bytes = fs::read(source)
        .map_err(|error| package_error("PACKAGE_READ_FAILED", "$source", error.to_string()))?;
    let legacy: CaseDefinition = serde_json::from_slice(&bytes).map_err(|error| {
        package_error("PACKAGE_LEGACY_CASE_INVALID", "$source", error.to_string())
    })?;
    legacy.validate().map_err(|errors| {
        package_error(
            "PACKAGE_LEGACY_CASE_INVALID",
            "$source",
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let template = adapt_v01(legacy, "1.0.0", VariantId::from("classic")).map_err(|error| {
        package_error(
            "PACKAGE_LEGACY_MIGRATION_FAILED",
            "$source",
            error.to_string(),
        )
    })?;
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
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| package_error("PACKAGE_WRITE_FAILED", "$output", error.to_string()))?;
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("case");
    let temporary = parent.join(format!(".{name}.tmp-{}", uuid::Uuid::new_v4()));
    let write_result = (|| -> Result<(), PackageError> {
        fs::create_dir(&temporary)
            .map_err(|error| package_error("PACKAGE_WRITE_FAILED", "$output", error.to_string()))?;
        fs::write(
            temporary.join("case.json"),
            serde_json::to_vec_pretty(&template).map_err(|error| {
                package_error("PACKAGE_WRITE_FAILED", "case.json", error.to_string())
            })?,
        )
        .map_err(|error| package_error("PACKAGE_WRITE_FAILED", "case.json", error.to_string()))?;
        fs::write(
            temporary.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).map_err(|error| {
                package_error("PACKAGE_WRITE_FAILED", "manifest.json", error.to_string())
            })?,
        )
        .map_err(|error| {
            package_error("PACKAGE_WRITE_FAILED", "manifest.json", error.to_string())
        })?;
        load_case_package(&temporary)?;
        fs::rename(&temporary, output)
            .map_err(|error| package_error("PACKAGE_WRITE_FAILED", "$output", error.to_string()))?;
        Ok(())
    })();
    if let Err(error) = write_result {
        fs::remove_dir_all(&temporary).ok();
        return Err(error);
    }
    load_case_package(output)
}

pub fn install_inline_package(
    manifest: &CaseManifest,
    template: &CaseTemplate,
    generation_report: Option<&serde_json::Value>,
    install_root: impl AsRef<Path>,
) -> Result<LoadedCasePackage, PackageError> {
    install_inline_package_with_visuals(manifest, template, generation_report, &[], install_root)
}

pub fn install_inline_package_with_visuals(
    manifest: &CaseManifest,
    template: &CaseTemplate,
    generation_report: Option<&serde_json::Value>,
    visuals: &[GeneratedVisualOutput],
    install_root: impl AsRef<Path>,
) -> Result<LoadedCasePackage, PackageError> {
    validate_manifest_shape(manifest)?;
    if !manifest.assets.is_empty() {
        return Err(package_error(
            "PACKAGE_ASSETS_UPLOAD_UNSUPPORTED",
            "manifest.assets",
            "inline installation does not accept asset files",
        ));
    }
    if manifest.visual_assets.len() != visuals.len()
        || visuals.iter().any(|output| {
            !manifest.visual_assets.iter().any(|entry| {
                entry.id == output.manifest.id
                    && entry.path == output.manifest.path
                    && entry.content_hash == output.manifest.content_hash
            })
        })
    {
        return Err(package_error(
            "PACKAGE_VISUAL_ASSET_SET_MISMATCH",
            "manifest.visual_assets",
            "visual manifest entries must exactly match uploaded visual outputs",
        ));
    }
    if manifest.id.as_ref().is_empty()
        || !manifest
            .id
            .as_ref()
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(package_error(
            "PACKAGE_PATH_COMPONENT_INVALID",
            "manifest.id",
            "value may contain only ASCII letters, digits, hyphen, and underscore",
        ));
    }
    if manifest.entry != "case.json" {
        return Err(package_error(
            "PACKAGE_INLINE_ENTRY_UNSUPPORTED",
            "manifest.entry",
            "inline installation requires entry to be case.json",
        ));
    }
    if manifest.generated && generation_report.is_none() {
        return Err(package_error(
            "PACKAGE_GENERATION_REPORT_REQUIRED",
            "generation_report",
            "generated package requires a generation report",
        ));
    }
    let install_root = install_root.as_ref();
    let case_root = install_root.join(manifest.id.as_ref());
    let destination = case_root.join(&manifest.version);
    if destination.exists() {
        let existing = load_case_package(&destination)?;
        let incoming_hash = canonical_hash(&(
            template,
            &manifest.schema_version,
            Vec::<(&str, &str)>::new(),
        ))
        .map_err(hash_error)?;
        if existing.template_content_hash == incoming_hash {
            return Ok(existing);
        }
        return Err(package_error(
            "PACKAGE_OUTPUT_EXISTS",
            "$install",
            "same case version is already installed with different content",
        ));
    }
    fs::create_dir_all(&case_root)
        .map_err(|error| package_error("PACKAGE_WRITE_FAILED", "$install", error.to_string()))?;
    let temporary = case_root.join(format!(
        ".{}.tmp-{}",
        manifest.version,
        uuid::Uuid::new_v4()
    ));
    let write_result = (|| -> Result<(), PackageError> {
        fs::create_dir(&temporary).map_err(|error| {
            package_error("PACKAGE_WRITE_FAILED", "$install", error.to_string())
        })?;
        write_json(&temporary.join("manifest.json"), manifest, "manifest.json")?;
        write_json(&temporary.join(&manifest.entry), template, &manifest.entry)?;
        if let Some(report) = generation_report {
            write_json(
                &temporary.join("generation-report.json"),
                report,
                "generation-report.json",
            )?;
        }
        for output in visuals {
            let relative =
                safe_relative_path(&output.manifest.path, "manifest.visual_assets.path")?;
            let destination = temporary.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    package_error(
                        "PACKAGE_WRITE_FAILED",
                        "manifest.visual_assets.path",
                        error.to_string(),
                    )
                })?;
            }
            fs::write(&destination, &output.bytes).map_err(|error| {
                package_error(
                    "PACKAGE_WRITE_FAILED",
                    "manifest.visual_assets.path",
                    error.to_string(),
                )
            })?;
        }
        load_case_package(&temporary)?;
        fs::rename(&temporary, &destination).map_err(|error| {
            package_error("PACKAGE_WRITE_FAILED", "$install", error.to_string())
        })?;
        Ok(())
    })();
    if let Err(error) = write_result {
        fs::remove_dir_all(&temporary).ok();
        return Err(error);
    }
    load_case_package(destination)
}

fn safe_relative_path<'a>(value: &'a str, field: &str) -> Result<&'a Path, PackageError> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(package_error(
            "PACKAGE_PATH_UNSAFE",
            field,
            "path must remain inside package",
        ));
    }
    Ok(path)
}

fn write_json(path: &Path, value: &impl serde::Serialize, field: &str) -> Result<(), PackageError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| package_error("PACKAGE_WRITE_FAILED", field, error.to_string()))?;
    fs::write(path, bytes)
        .map_err(|error| package_error("PACKAGE_WRITE_FAILED", field, error.to_string()))
}

fn validate_manifest_shape(manifest: &CaseManifest) -> Result<(), PackageError> {
    if manifest.schema_version != "0.2" {
        return Err(package_error(
            "PACKAGE_SCHEMA_VERSION_UNSUPPORTED",
            "manifest.json.schema_version",
            "expected schema version 0.2",
        ));
    }
    let parts: Vec<_> = manifest.version.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || part.parse::<u64>().is_err())
    {
        return Err(package_error(
            "PACKAGE_VERSION_INVALID",
            "manifest.json.version",
            "restricted version must use numeric major.minor.patch",
        ));
    }
    Ok(())
}

fn validate_manifest_identity(
    manifest: &CaseManifest,
    template: &CaseTemplate,
) -> Result<(), PackageError> {
    let checks = [
        (
            manifest.id.as_ref() == template.id.as_ref(),
            "manifest.json.id",
            "manifest ID differs from template ID",
        ),
        (
            manifest.version == template.version,
            "manifest.json.version",
            "manifest version differs from template version",
        ),
        (
            manifest.schema_version == template.schema_version,
            "manifest.json.schema_version",
            "manifest schema differs from template schema",
        ),
        (
            manifest.default_variant_id == template.default_variant_id,
            "manifest.json.default_variant_id",
            "manifest default variant differs from template",
        ),
        (
            manifest.variant_count as usize == template.solution_variants.len(),
            "manifest.json.variant_count",
            "manifest variant count differs from template",
        ),
    ];
    for (valid, path, message) in checks {
        if !valid {
            return Err(package_error("PACKAGE_IDENTITY_MISMATCH", path, message));
        }
    }
    Ok(())
}

fn safe_file(root: &Path, relative: &str, field: &str) -> Result<PathBuf, PackageError> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(package_error(
            "PACKAGE_PATH_UNSAFE",
            field,
            "path must be a non-empty relative path without traversal",
        ));
    }
    let mut current = root.to_path_buf();
    for component in relative_path.components() {
        let Component::Normal(segment) = component else {
            unreachable!("validated above")
        };
        current.push(segment);
        reject_symlink(&current, field)?;
    }
    if !current.is_file() {
        return Err(package_error(
            "PACKAGE_FILE_MISSING",
            field,
            format!("{} is not a file", current.display()),
        ));
    }
    Ok(current)
}

fn reject_symlink(path: &Path, field: &str) -> Result<(), PackageError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| package_error("PACKAGE_FILE_MISSING", field, error.to_string()))?;
    if metadata.file_type().is_symlink() {
        return Err(package_error(
            "PACKAGE_SYMLINK_FORBIDDEN",
            field,
            format!("{} is a symbolic link", path.display()),
        ));
    }
    Ok(())
}

fn hash_error(error: HashError) -> PackageError {
    package_error("PACKAGE_HASH_FAILED", "$", error.to_string())
}

fn package_error(
    code: &'static str,
    path: impl Into<String>,
    message: impl Into<String>,
) -> PackageError {
    PackageError {
        code,
        path: path.into(),
        message: message.into(),
    }
}
