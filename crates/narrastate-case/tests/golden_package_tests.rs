use narrastate_case::load_case_package;

#[test]
fn checked_in_three_variant_package_compiles_validates_and_simulates() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/rain-gallery-variants");
    let package = load_case_package(root).expect("checked-in Golden Package");
    assert_eq!(package.manifest.variant_count, 3);
    assert_eq!(package.validation.variant_reports.len(), 3);
    for report in package.validation.variant_reports {
        assert!(report.valid, "variant {}", report.variant_id);
        assert!(report.simulation.expect("simulation report").success);
    }
}

#[test]
fn checked_in_invalid_packages_fail_with_their_stable_expected_codes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/golden-invalid");
    for entry in std::fs::read_dir(root).expect("invalid Golden root") {
        let directory = entry.expect("directory entry").path();
        if !directory.is_dir() {
            continue;
        }
        let expected: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("expected.json")).expect("expected codes"),
        )
        .expect("expected JSON");
        let codes = expected["codes"].as_array().expect("codes array");
        let error = load_case_package(&directory).expect_err("invalid package must fail");
        assert_eq!(
            error.code,
            "PACKAGE_CASE_INVALID",
            "{}",
            directory.display()
        );
        assert!(
            codes
                .iter()
                .any(|code| error.message.contains(code.as_str().expect("code string"))),
            "{} returned: {}",
            directory.display(),
            error.message
        );
    }
}
