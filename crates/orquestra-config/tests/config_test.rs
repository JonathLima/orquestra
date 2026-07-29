#[test]
fn load_config_toml_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
config_version = 1

[[verification.profiles]]
name = "unit-test"
argv = ["cargo", "test", "--quiet"]
relative_dir = "../"
timeout_seconds = 180
max_output_bytes = 524288
expected_exit_code = 0
"#,
    )
    .unwrap();

    let config = orquestra_config::OrquestraConfig::load(&path).unwrap();
    assert_eq!(config.config_version, 1);
    assert_eq!(config.verification.profiles.len(), 1);
    assert_eq!(config.verification.profiles[0].name, "unit-test");
}

#[test]
fn execute_simple_profile() {
    let dir = tempfile::tempdir().unwrap();
    let profile = orquestra_config::VerificationProfile {
        name: "check".to_string(),
        argv: if cfg!(target_os = "windows") {
            vec![
                "cmd".to_string(),
                "/c".to_string(),
                "echo".to_string(),
                "ok".to_string(),
            ]
        } else {
            vec!["echo".to_string(), "ok".to_string()]
        },
        relative_dir: None,
        timeout_seconds: None,
        max_output_bytes: None,
        expected_exit_code: Some(0),
        expected_artifacts: vec![],
        env: vec![],
    };
    let result = orquestra_config::profile::execute_profile(&profile, dir.path()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("ok"), "stdout: {}", result.stdout);
}

#[test]
fn verify_bounded_output() {
    let dir = tempfile::tempdir().unwrap();
    let profile = orquestra_config::VerificationProfile {
        name: "bounded".to_string(),
        argv: if cfg!(target_os = "windows") {
            vec![
                "cmd".to_string(),
                "/c".to_string(),
                "echo".to_string(),
                "abcdefghij".to_string(),
            ]
        } else {
            vec!["echo".to_string(), "abcdefghij".to_string()]
        },
        relative_dir: None,
        timeout_seconds: None,
        max_output_bytes: Some(6),
        expected_exit_code: None,
        expected_artifacts: vec![],
        env: vec![],
    };
    let result = orquestra_config::profile::execute_profile(&profile, dir.path()).unwrap();
    let out = result.stdout.trim();
    assert!(
        out.len() <= 9,
        "output should be truncated, len={}",
        out.len()
    );
}

#[test]
fn profile_not_found_error_message() {
    let config = orquestra_config::OrquestraConfig::load_default();
    let profile = config.get_profile("nonexistent");
    assert!(profile.is_none());
}
