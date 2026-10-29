//! Every command in `generate_handler!` must be enrolled in build.rs and granted
//! to the main window in capabilities/main.json; otherwise the app rejects the
//! call at run time and the button that uses it silently does nothing.

use std::collections::BTreeSet;

fn quoted_names(text: &str) -> BTreeSet<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()))
        .map(str::to_string)
        .collect()
}

#[test]
fn handler_build_manifest_and_capability_agree() {
    let main = include_str!("../src/main.rs");
    let start = main.find("generate_handler![").expect("generate_handler!");
    let body = &main[start + "generate_handler![".len()..];
    let body = &body[..body.find(']').expect("end of handler list")];
    let handler: BTreeSet<String> = body
        .split(',')
        .map(|s| s.trim().rsplit("::").next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty() && !s.starts_with("//"))
        .collect();

    assert!(handler.len() > 50, "handler list not parsed: {}", handler.len());

    let build = include_str!("../build.rs");
    let manifest = quoted_names(build);
    let capability = include_str!("../capabilities/main.json");
    let granted: BTreeSet<String> = capability
        .split('"')
        .filter_map(|s| s.strip_prefix("allow-"))
        .map(|s| s.replace('-', "_"))
        .collect();

    let missing_manifest: Vec<_> = handler.difference(&manifest).collect();
    let missing_grant: Vec<_> = handler.difference(&granted).collect();
    assert!(missing_manifest.is_empty(), "not in build.rs COMMANDS: {missing_manifest:?}");
    assert!(missing_grant.is_empty(), "not granted in capabilities/main.json: {missing_grant:?}");
}
