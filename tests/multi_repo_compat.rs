//! Multi-repository binary comparison tests.
//!
//! Clone external repos at pinned commits, run matchum, and compare
//! against stored cspell snapshots.
//!
//! Run all comparisons:
//!   cargo test --test multi_repo_compat -- --ignored
//!
//! Generate/update all cspell snapshots (requires `npx cspell`):
//!   cargo test --test multi_repo_compat generate_all_snapshots -- --ignored
//!
//! Generate a single snapshot:
//!   cargo test --test multi_repo_compat generate_snapshot_rustpython -- --ignored

mod multi_repo;

use multi_repo::harness;
use multi_repo::repos;

// ---------------------------------------------------------------------------
// Comparison tests (matchum vs cspell snapshot)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn multi_repo_php_src() {
    harness::compare_repo(&repos::PHP_SRC);
}

#[test]
#[ignore]
fn multi_repo_specberus() {
    harness::compare_repo(&repos::SPECBERUS);
}

#[test]
#[ignore]
fn multi_repo_typescript_starter() {
    harness::compare_repo(&repos::TYPESCRIPT_STARTER);
}

#[test]
#[ignore]
fn multi_repo_licia() {
    harness::compare_repo(&repos::LICIA);
}

#[test]
#[ignore]
fn multi_repo_wire_webapp() {
    harness::compare_repo(&repos::WIRE_WEBAPP);
}

#[test]
#[ignore]
fn multi_repo_typescript_cheatsheets_react() {
    harness::compare_repo(&repos::TYPESCRIPT_CHEATSHEETS_REACT);
}

#[test]
#[ignore]
fn multi_repo_express_graphql() {
    harness::compare_repo(&repos::EXPRESS_GRAPHQL);
}

#[test]
#[ignore]
fn multi_repo_wire_desktop() {
    harness::compare_repo(&repos::WIRE_DESKTOP);
}

#[test]
#[ignore]
fn multi_repo_graphql_relay_js() {
    harness::compare_repo(&repos::GRAPHQL_RELAY_JS);
}

#[test]
#[ignore]
fn multi_repo_aws_amplify_docs() {
    harness::compare_repo(&repos::AWS_AMPLIFY_DOCS);
}

#[test]
#[ignore]
fn multi_repo_webpack_assets_manifest() {
    harness::compare_repo(&repos::WEBPACK_ASSETS_MANIFEST);
}

#[test]
#[ignore]
fn multi_repo_prettier() {
    harness::compare_repo(&repos::PRETTIER);
}

#[test]
#[ignore]
fn multi_repo_webpack() {
    harness::compare_repo(&repos::WEBPACK);
}

#[test]
#[ignore]
fn multi_repo_aria_practices() {
    harness::compare_repo(&repos::ARIA_PRACTICES);
}

#[test]
#[ignore]
fn multi_repo_admin_bro() {
    harness::compare_repo(&repos::ADMIN_BRO);
}

#[test]
#[ignore]
fn multi_repo_typescript_eslint() {
    harness::compare_repo(&repos::TYPESCRIPT_ESLINT);
}

#[test]
#[ignore]
fn multi_repo_graphql_js() {
    harness::compare_repo(&repos::GRAPHQL_JS);
}

#[test]
#[ignore]
fn multi_repo_typescript_website() {
    harness::compare_repo(&repos::TYPESCRIPT_WEBSITE);
}

#[test]
#[ignore]
fn multi_repo_azure_rest_api_specs() {
    harness::compare_repo(&repos::AZURE_REST_API_SPECS);
}

#[test]
#[ignore]
fn multi_repo_jira() {
    harness::compare_repo(&repos::JIRA);
}

#[test]
#[ignore]
fn multi_repo_exonum() {
    harness::compare_repo(&repos::EXONUM);
}

#[test]
#[ignore]
fn multi_repo_thealgorithms_python() {
    harness::compare_repo(&repos::THEALGORITHMS_PYTHON);
}

#[test]
#[ignore]
fn multi_repo_django() {
    harness::compare_repo(&repos::DJANGO);
}

#[test]
#[ignore]
fn multi_repo_megistos() {
    harness::compare_repo(&repos::MEGISTOS);
}

#[test]
#[ignore]
fn multi_repo_mdx() {
    harness::compare_repo(&repos::MDX);
}

#[test]
#[ignore]
fn multi_repo_adadoom3() {
    harness::compare_repo(&repos::ADADOOM3);
}

#[test]
#[ignore]
fn multi_repo_latex_examples() {
    harness::compare_repo(&repos::LATEX_EXAMPLES);
}

#[test]
#[ignore]
fn multi_repo_google_cloud_cpp() {
    harness::compare_repo(&repos::GOOGLE_CLOUD_CPP);
}

#[test]
#[ignore]
fn multi_repo_graphql_spec() {
    harness::compare_repo(&repos::GRAPHQL_SPEC);
}

#[test]
#[ignore]
fn multi_repo_tplink_smarthome_api() {
    harness::compare_repo(&repos::TPLINK_SMARTHOME_API);
}

#[test]
#[ignore]
fn multi_repo_open_source_logiciel_libre() {
    harness::compare_repo(&repos::OPEN_SOURCE_LOGICIEL_LIBRE);
}

#[test]
#[ignore]
fn multi_repo_pagekit() {
    harness::compare_repo(&repos::PAGEKIT);
}

#[test]
#[ignore]
fn multi_repo_bootstrap() {
    harness::compare_repo(&repos::BOOTSTRAP);
}

#[test]
#[ignore]
fn multi_repo_apollo_server() {
    harness::compare_repo(&repos::APOLLO_SERVER);
}

#[test]
#[ignore]
fn multi_repo_shoelace() {
    harness::compare_repo(&repos::SHOELACE);
}

#[test]
#[ignore]
fn multi_repo_aspnetboilerplate() {
    harness::compare_repo(&repos::ASPNETBOILERPLATE);
}

#[test]
#[ignore]
fn multi_repo_caddy() {
    harness::compare_repo(&repos::CADDY);
}

#[test]
#[ignore]
fn multi_repo_eslint() {
    harness::compare_repo(&repos::ESLINT);
}

#[test]
#[ignore]
fn multi_repo_java_design_patterns() {
    harness::compare_repo(&repos::JAVA_DESIGN_PATTERNS);
}

#[test]
#[ignore]
fn multi_repo_sqlserver_kit() {
    harness::compare_repo(&repos::SQLSERVER_KIT);
}

#[test]
#[ignore]
fn multi_repo_svelte() {
    harness::compare_repo(&repos::SVELTE);
}

#[test]
#[ignore]
fn multi_repo_rustpython() {
    harness::compare_repo(&repos::RUSTPYTHON);
}

#[test]
#[ignore]
fn multi_repo_chef() {
    harness::compare_repo(&repos::CHEF);
}

#[test]
#[ignore]
fn multi_repo_gitbucket() {
    harness::compare_repo(&repos::GITBUCKET);
}

#[test]
#[ignore]
fn multi_repo_nvim_lspconfig() {
    harness::compare_repo(&repos::NVIM_LSPCONFIG);
}

#[test]
#[ignore]
fn multi_repo_powershell_docs() {
    harness::compare_repo(&repos::POWERSHELL_DOCS);
}

#[test]
#[ignore]
fn multi_repo_vitest() {
    harness::compare_repo(&repos::VITEST);
}

#[test]
#[ignore]
fn multi_repo_flutter_samples() {
    harness::compare_repo(&repos::FLUTTER_SAMPLES);
}

#[test]
#[ignore]
fn multi_repo_dart_sdk() {
    harness::compare_repo(&repos::DART_SDK);
}

#[test]
#[ignore]
fn multi_repo_slint() {
    harness::compare_repo(&repos::SLINT);
}

// ---------------------------------------------------------------------------
// Snapshot generators (requires `npx cspell`)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn generate_snapshot_php_src() {
    harness::generate_snapshot(&repos::PHP_SRC);
}

#[test]
#[ignore]
fn generate_snapshot_specberus() {
    harness::generate_snapshot(&repos::SPECBERUS);
}

#[test]
#[ignore]
fn generate_snapshot_typescript_starter() {
    harness::generate_snapshot(&repos::TYPESCRIPT_STARTER);
}

#[test]
#[ignore]
fn generate_snapshot_licia() {
    harness::generate_snapshot(&repos::LICIA);
}

#[test]
#[ignore]
fn generate_snapshot_wire_webapp() {
    harness::generate_snapshot(&repos::WIRE_WEBAPP);
}

#[test]
#[ignore]
fn generate_snapshot_typescript_cheatsheets_react() {
    harness::generate_snapshot(&repos::TYPESCRIPT_CHEATSHEETS_REACT);
}

#[test]
#[ignore]
fn generate_snapshot_express_graphql() {
    harness::generate_snapshot(&repos::EXPRESS_GRAPHQL);
}

#[test]
#[ignore]
fn generate_snapshot_wire_desktop() {
    harness::generate_snapshot(&repos::WIRE_DESKTOP);
}

#[test]
#[ignore]
fn generate_snapshot_graphql_relay_js() {
    harness::generate_snapshot(&repos::GRAPHQL_RELAY_JS);
}

#[test]
#[ignore]
fn generate_snapshot_aws_amplify_docs() {
    harness::generate_snapshot(&repos::AWS_AMPLIFY_DOCS);
}

#[test]
#[ignore]
fn generate_snapshot_webpack_assets_manifest() {
    harness::generate_snapshot(&repos::WEBPACK_ASSETS_MANIFEST);
}

#[test]
#[ignore]
fn generate_snapshot_prettier() {
    harness::generate_snapshot(&repos::PRETTIER);
}

#[test]
#[ignore]
fn generate_snapshot_webpack() {
    harness::generate_snapshot(&repos::WEBPACK);
}

#[test]
#[ignore]
fn generate_snapshot_aria_practices() {
    harness::generate_snapshot(&repos::ARIA_PRACTICES);
}

#[test]
#[ignore]
fn generate_snapshot_admin_bro() {
    harness::generate_snapshot(&repos::ADMIN_BRO);
}

#[test]
#[ignore]
fn generate_snapshot_typescript_eslint() {
    harness::generate_snapshot(&repos::TYPESCRIPT_ESLINT);
}

#[test]
#[ignore]
fn generate_snapshot_graphql_js() {
    harness::generate_snapshot(&repos::GRAPHQL_JS);
}

#[test]
#[ignore]
fn generate_snapshot_typescript_website() {
    harness::generate_snapshot(&repos::TYPESCRIPT_WEBSITE);
}

#[test]
#[ignore]
fn generate_snapshot_azure_rest_api_specs() {
    harness::generate_snapshot(&repos::AZURE_REST_API_SPECS);
}

#[test]
#[ignore]
fn generate_snapshot_jira() {
    harness::generate_snapshot(&repos::JIRA);
}

#[test]
#[ignore]
fn generate_snapshot_exonum() {
    harness::generate_snapshot(&repos::EXONUM);
}

#[test]
#[ignore]
fn generate_snapshot_thealgorithms_python() {
    harness::generate_snapshot(&repos::THEALGORITHMS_PYTHON);
}

#[test]
#[ignore]
fn generate_snapshot_django() {
    harness::generate_snapshot(&repos::DJANGO);
}

#[test]
#[ignore]
fn generate_snapshot_megistos() {
    harness::generate_snapshot(&repos::MEGISTOS);
}

#[test]
#[ignore]
fn generate_snapshot_mdx() {
    harness::generate_snapshot(&repos::MDX);
}

#[test]
#[ignore]
fn generate_snapshot_adadoom3() {
    harness::generate_snapshot(&repos::ADADOOM3);
}

#[test]
#[ignore]
fn generate_snapshot_latex_examples() {
    harness::generate_snapshot(&repos::LATEX_EXAMPLES);
}

#[test]
#[ignore]
fn generate_snapshot_google_cloud_cpp() {
    harness::generate_snapshot(&repos::GOOGLE_CLOUD_CPP);
}

#[test]
#[ignore]
fn generate_snapshot_graphql_spec() {
    harness::generate_snapshot(&repos::GRAPHQL_SPEC);
}

#[test]
#[ignore]
fn generate_snapshot_tplink_smarthome_api() {
    harness::generate_snapshot(&repos::TPLINK_SMARTHOME_API);
}

#[test]
#[ignore]
fn generate_snapshot_open_source_logiciel_libre() {
    harness::generate_snapshot(&repos::OPEN_SOURCE_LOGICIEL_LIBRE);
}

#[test]
#[ignore]
fn generate_snapshot_pagekit() {
    harness::generate_snapshot(&repos::PAGEKIT);
}

#[test]
#[ignore]
fn generate_snapshot_bootstrap() {
    harness::generate_snapshot(&repos::BOOTSTRAP);
}

#[test]
#[ignore]
fn generate_snapshot_apollo_server() {
    harness::generate_snapshot(&repos::APOLLO_SERVER);
}

#[test]
#[ignore]
fn generate_snapshot_shoelace() {
    harness::generate_snapshot(&repos::SHOELACE);
}

#[test]
#[ignore]
fn generate_snapshot_aspnetboilerplate() {
    harness::generate_snapshot(&repos::ASPNETBOILERPLATE);
}

#[test]
#[ignore]
fn generate_snapshot_caddy() {
    harness::generate_snapshot(&repos::CADDY);
}

#[test]
#[ignore]
fn generate_snapshot_eslint() {
    harness::generate_snapshot(&repos::ESLINT);
}

#[test]
#[ignore]
fn generate_snapshot_java_design_patterns() {
    harness::generate_snapshot(&repos::JAVA_DESIGN_PATTERNS);
}

#[test]
#[ignore]
fn generate_snapshot_sqlserver_kit() {
    harness::generate_snapshot(&repos::SQLSERVER_KIT);
}

#[test]
#[ignore]
fn generate_snapshot_svelte() {
    harness::generate_snapshot(&repos::SVELTE);
}

#[test]
#[ignore]
fn generate_snapshot_rustpython() {
    harness::generate_snapshot(&repos::RUSTPYTHON);
}

#[test]
#[ignore]
fn generate_snapshot_chef() {
    harness::generate_snapshot(&repos::CHEF);
}

#[test]
#[ignore]
fn generate_snapshot_gitbucket() {
    harness::generate_snapshot(&repos::GITBUCKET);
}

#[test]
#[ignore]
fn generate_snapshot_nvim_lspconfig() {
    harness::generate_snapshot(&repos::NVIM_LSPCONFIG);
}

#[test]
#[ignore]
fn generate_snapshot_powershell_docs() {
    harness::generate_snapshot(&repos::POWERSHELL_DOCS);
}

#[test]
#[ignore]
fn generate_snapshot_vitest() {
    harness::generate_snapshot(&repos::VITEST);
}

#[test]
#[ignore]
fn generate_snapshot_flutter_samples() {
    harness::generate_snapshot(&repos::FLUTTER_SAMPLES);
}

#[test]
#[ignore]
fn generate_snapshot_dart_sdk() {
    harness::generate_snapshot(&repos::DART_SDK);
}

#[test]
#[ignore]
fn generate_snapshot_slint() {
    harness::generate_snapshot(&repos::SLINT);
}

// ---------------------------------------------------------------------------
// Bulk operations
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn generate_all_snapshots() {
    for spec in repos::REPOS {
        harness::generate_snapshot(spec);
    }
}
