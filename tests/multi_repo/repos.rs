use super::harness::RepoSpec;

pub const PHP_SRC: RepoSpec = RepoSpec {
    name: "php-src",
    url: "https://github.com/php/php-src.git",
    commit: "ffd58ea601c1cdbf95e4a8e35c07841bf8395d13",
    check_paths: &["**/*.{md,c,h,php}"],
    cspell_config: None,
    args: &[],
};

pub const SPECBERUS: RepoSpec = RepoSpec {
    name: "specberus",
    url: "https://github.com/w3c/specberus.git",
    commit: "2dcc9044623bac1f19d99a2b85bf7495d2913bd9",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const TYPESCRIPT_STARTER: RepoSpec = RepoSpec {
    name: "typescript-starter",
    url: "https://github.com/bitjson/typescript-starter.git",
    commit: "586cdb3029ab2c52e2f0893adafbbb017059e1c9",
    check_paths: &["{README.md,.github/*.md,src/**/*.ts}"],
    cspell_config: None,
    args: &[],
};

pub const LICIA: RepoSpec = RepoSpec {
    name: "licia",
    url: "https://github.com/liriliri/licia.git",
    commit: "ce21a9276e39aaedcbcec71a8e17871e23beeae9",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const WIRE_WEBAPP: RepoSpec = RepoSpec {
    name: "wire-webapp",
    url: "https://github.com/wireapp/wire-webapp.git",
    commit: "a889a1b63154e92e213fcf2619a02733ff4c58b8",
    check_paths: &["{apps,docs}/**/*.{js,ts,tsx,md,mjs,mts,cjs,cts}"],
    cspell_config: None,
    args: &[],
};

pub const TYPESCRIPT_CHEATSHEETS_REACT: RepoSpec = RepoSpec {
    name: "typescript-cheatsheets-react",
    url: "https://github.com/typescript-cheatsheets/react.git",
    commit: "c299ceb2d0d26cdff06b2350b2ed6fd8d2449cc0",
    check_paths: &["**/*.{ts,js,md}"],
    cspell_config: None,
    args: &[],
};

pub const EXPRESS_GRAPHQL: RepoSpec = RepoSpec {
    name: "express-graphql",
    url: "https://github.com/graphql/express-graphql.git",
    commit: "3fab4b1e016cd27655f3b013f65a6b1344520d01",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const WIRE_DESKTOP: RepoSpec = RepoSpec {
    name: "wire-desktop",
    url: "https://github.com/wireapp/wire-desktop.git",
    commit: "6e2330ce939d5c6b3366af026ecb6001c31a5fc3",
    check_paths: &[
        "*.md",
        "electron/renderer/**/*.jsx",
        "electron/src/**/*.ts",
        "electron/html/*.html",
    ],
    cspell_config: None,
    args: &[],
};

pub const GRAPHQL_RELAY_JS: RepoSpec = RepoSpec {
    name: "graphql-relay-js",
    url: "https://github.com/graphql/graphql-relay-js.git",
    commit: "6600e95a3cfebebfc40e746425fe278c8aa9da7b",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const AWS_AMPLIFY_DOCS: RepoSpec = RepoSpec {
    name: "aws-amplify-docs",
    url: "https://github.com/aws-amplify/docs.git",
    commit: "c727fecb6106b07c5e935591dc22f6d1b0374500",
    check_paths: &["**/*.{md,mdx}"],
    cspell_config: None,
    args: &[],
};

pub const WEBPACK_ASSETS_MANIFEST: RepoSpec = RepoSpec {
    name: "webpack-assets-manifest",
    url: "https://github.com/webdeveric/webpack-assets-manifest.git",
    commit: "91fe1d72765aa763d8db4cd10d332abe8ad0d649",
    check_paths: &["."],
    cspell_config: None,
    args: &[],
};

pub const PRETTIER: RepoSpec = RepoSpec {
    name: "prettier",
    url: "https://github.com/prettier/prettier.git",
    commit: "9bb7335452738e54ace26093bf9158c033bbfe7b",
    check_paths: &[],
    cspell_config: None,
    args: &[],
};

pub const WEBPACK: RepoSpec = RepoSpec {
    name: "webpack",
    url: "https://github.com/webpack/webpack.git",
    commit: "40989026fa542ceffa2821e5d25ae0d87c0bd09b",
    check_paths: &[
        "{.github,benchmark,bin,examples,hot,lib,schemas,setup,tooling}/**/*.{md,yml,yaml,js,json}",
        "*.md",
    ],
    cspell_config: None,
    args: &[],
};

pub const ARIA_PRACTICES: RepoSpec = RepoSpec {
    name: "aria-practices",
    url: "https://github.com/w3c/aria-practices.git",
    commit: "d847aad4a09e8a75f8af0d205f178d59807d2322",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const ADMIN_BRO: RepoSpec = RepoSpec {
    name: "admin-bro",
    url: "https://github.com/SoftwareBrothers/admin-bro.git",
    commit: "9c2ee7f1b58c1471edc02f39d01db64d0764873e",
    check_paths: &["src/**/*.{ts,js,tsx,jsx}", "**/*.md"],
    cspell_config: None,
    args: &[],
};

pub const TYPESCRIPT_ESLINT: RepoSpec = RepoSpec {
    name: "typescript-eslint",
    url: "https://github.com/typescript-eslint/typescript-eslint.git",
    commit: "a09921e2de2e8790e6a803016b825815ca9409d8",
    check_paths: &["**/*.{md,ts,js}"],
    cspell_config: Some(".cspell.json"),
    args: &[],
};

pub const GRAPHQL_JS: RepoSpec = RepoSpec {
    name: "graphql-js",
    url: "https://github.com/graphql/graphql-js.git",
    commit: "7569989a8850c00a4e90ffdb56f1c6094cdcd795",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const TYPESCRIPT_WEBSITE: RepoSpec = RepoSpec {
    name: "typescript-website",
    url: "https://github.com/microsoft/TypeScript-Website.git",
    commit: "6a12128c9ee429a39e41981473f89a88b5cec7b3",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const AZURE_REST_API_SPECS: RepoSpec = RepoSpec {
    name: "azure-rest-api-specs",
    url: "https://github.com/Azure/azure-rest-api-specs.git",
    commit: "a41799da220be570a0fed686bf08e98ee2738424",
    check_paths: &["**/*.{md,ts,js}"],
    cspell_config: Some("cspell.json"),
    args: &[],
};

pub const JIRA: RepoSpec = RepoSpec {
    name: "jira",
    url: "https://github.com/pycontribs/jira.git",
    commit: "b07a89bc3401094fdc4c2c2c6f125855fde0a10e",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const EXONUM: RepoSpec = RepoSpec {
    name: "exonum",
    url: "https://github.com/exonum/exonum.git",
    commit: "2d2fa22e5f5bc451d08c155c2398956f11dce06e",
    check_paths: &["**/*.{rs,md,py,proto}"],
    cspell_config: None,
    args: &[],
};

pub const THEALGORITHMS_PYTHON: RepoSpec = RepoSpec {
    name: "thealgorithms-python",
    url: "https://github.com/TheAlgorithms/Python.git",
    commit: "678dedbbf94be54b3c9c258368e28bb8e7736d62",
    check_paths: &["**/*.{md,py}"],
    cspell_config: None,
    args: &[],
};

pub const DJANGO: RepoSpec = RepoSpec {
    name: "django",
    url: "https://github.com/django/django.git",
    commit: "202eca7795710c82cd64a248650afc64f905a17e",
    check_paths: &["**/*.{md,py}"],
    cspell_config: None,
    args: &[],
};

pub const MEGISTOS: RepoSpec = RepoSpec {
    name: "megistos",
    url: "https://github.com/alexiosc/megistos.git",
    commit: "17439f76a2ea1608fe6f0addc3c671eefa0424ce",
    check_paths: &["**/*.{md,c,h,html}"],
    cspell_config: None,
    args: &[],
};

pub const MDX: RepoSpec = RepoSpec {
    name: "mdx",
    url: "https://github.com/mdx-js/mdx",
    commit: "b4110d2b740c2680381d410a9003c48752863f92",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const ADADOOM3: RepoSpec = RepoSpec {
    name: "adadoom3",
    url: "https://github.com/AdaDoom3/AdaDoom3.git",
    commit: "6b979248c03cfae9af705257b116179f7d250267",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const LATEX_EXAMPLES: RepoSpec = RepoSpec {
    name: "latex-examples",
    url: "https://github.com/MartinThoma/LaTeX-examples.git",
    commit: "2286e6e3833904b2c058b2a855db9b7f81776c59",
    check_paths: &["**/*.{md,tex}"],
    cspell_config: None,
    args: &[],
};

pub const GOOGLE_CLOUD_CPP: RepoSpec = RepoSpec {
    name: "google-cloud-cpp",
    url: "https://github.com/googleapis/google-cloud-cpp.git",
    commit: "07a9fce6f4c98cef6fd2f9ba47b66ff3a67b2600",
    check_paths: &["**/*"],
    cspell_config: None,
    args: &[
        "--exclude={*.BUILD,BUILD,CHANGELOG.md,*.sh,*.cfg,*.ps1,Dockerfile.*,*.Dockerfile,*.{yaml,xml,json,cmake}}",
    ],
};

pub const GRAPHQL_SPEC: RepoSpec = RepoSpec {
    name: "graphql-spec",
    url: "https://github.com/graphql/graphql-spec.git",
    commit: "ff0d285289e1bd9c8c38d49743f94a90ce3e0bf3",
    check_paths: &["**/*.md"],
    cspell_config: None,
    args: &[],
};

pub const TPLINK_SMARTHOME_API: RepoSpec = RepoSpec {
    name: "tplink-smarthome-api",
    url: "https://github.com/plasticrake/tplink-smarthome-api.git",
    commit: "33f55531e6d5935d57a065fb95fa5dc340c4f392",
    check_paths: &["{examples,src,test}/**/*", "**/*.md"],
    cspell_config: None,
    args: &[],
};

pub const OPEN_SOURCE_LOGICIEL_LIBRE: RepoSpec = RepoSpec {
    name: "open-source-logiciel-libre",
    url: "https://github.com/canada-ca/open-source-logiciel-libre.git",
    commit: "fac6c4a53dfd0b160238699587e7d001bc5a01f1",
    check_paths: &["en/**/*.md"],
    cspell_config: None,
    args: &[],
};

pub const PAGEKIT: RepoSpec = RepoSpec {
    name: "pagekit",
    url: "https://github.com/pagekit/pagekit.git",
    commit: "6fc7aa13653a35c29217d031d8ab09b2a1124f2d",
    check_paths: &["**"],
    cspell_config: None,
    args: &[
        "--exclude=/app/assets/codemirror",
        "--exclude=/app/system/languages",
    ],
};

pub const BOOTSTRAP: RepoSpec = RepoSpec {
    name: "bootstrap",
    url: "https://github.com/twbs/bootstrap.git",
    commit: "c82919e8970c7ebebb87ded191b815c76737cd04",
    check_paths: &["site/**/*.{md,mdx}"],
    cspell_config: None,
    args: &[],
};

pub const APOLLO_SERVER: RepoSpec = RepoSpec {
    name: "apollo-server",
    url: "https://github.com/apollographql/apollo-server.git",
    commit: "e66bd0bfc235bba7d55ec7b353f65aee865448f1",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const SHOELACE: RepoSpec = RepoSpec {
    name: "shoelace",
    url: "https://github.com/shoelace-style/shoelace.git",
    commit: "87cae5de35bc0554dbaae531efe2852e4bf06b8f",
    check_paths: &["**"],
    cspell_config: None,
    args: &["--exclude=docs/assets/**"],
};

pub const ASPNETBOILERPLATE: RepoSpec = RepoSpec {
    name: "aspnetboilerplate",
    url: "https://github.com/aspnetboilerplate/aspnetboilerplate",
    commit: "c727b75bfc694cddea1ccb2235720a842df6f58d",
    check_paths: &["**/*.{md,cs,cshtml}"],
    cspell_config: None,
    args: &["--exclude=wwwroot", "--exclude=**/*SampleApp.Tests/Web*"],
};

pub const CADDY: RepoSpec = RepoSpec {
    name: "caddy",
    url: "https://github.com/caddyserver/caddy.git",
    commit: "2ab043b8903db4574b2fc7a625619018a072f082",
    check_paths: &["**/*.go"],
    cspell_config: None,
    args: &[],
};

pub const ESLINT: RepoSpec = RepoSpec {
    name: "eslint",
    url: "https://github.com/eslint/eslint",
    commit: "53ca6eeed87262ebddd20636107f486badabcc1f",
    check_paths: &["."],
    cspell_config: None,
    args: &[
        "--exclude=bin/**",
        "--exclude=CHANGELOG.md",
        "--exclude=_data",
        "--exclude=tests/bench/large.js",
        "--exclude=docs/src/_includes",
        "--exclude=docs/src/assets/{fonts,s?css,images}",
    ],
};

pub const JAVA_DESIGN_PATTERNS: RepoSpec = RepoSpec {
    name: "java-design-patterns",
    url: "https://github.com/iluwatar/java-design-patterns.git",
    commit: "2d39fe7ed8acfb683890644355ab5156c9b37354",
    check_paths: &["**/*.md", "**/*.java"],
    cspell_config: None,
    args: &[],
};

pub const SQLSERVER_KIT: RepoSpec = RepoSpec {
    name: "sqlserver-kit",
    url: "https://github.com/ktaranov/sqlserver-kit.git",
    commit: "fa7635fc0a89baedafc01f38bacee5c4cbe214f2",
    check_paths: &["**"],
    cspell_config: None,
    args: &["--exclude=**/Backup/**"],
};

pub const SVELTE: RepoSpec = RepoSpec {
    name: "svelte",
    url: "https://github.com/sveltejs/svelte.git",
    commit: "4aa3777271c444022e52cceba775fc9e82a42fa4",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const RUSTPYTHON: RepoSpec = RepoSpec {
    name: "rustpython",
    url: "https://github.com/RustPython/RustPython.git",
    commit: "baba1f9447fd287e9b3386cc0f3af111e617e9ec",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const CHEF: RepoSpec = RepoSpec {
    name: "chef",
    url: "https://github.com/chef/chef.git",
    commit: "f34c6572aa497124927e795dff129025533616bb",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const GITBUCKET: RepoSpec = RepoSpec {
    name: "gitbucket",
    url: "https://github.com/gitbucket/gitbucket.git",
    commit: "cce9c625916093bf9f8f972208f56816ece27d69",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const NVIM_LSPCONFIG: RepoSpec = RepoSpec {
    name: "nvim-lspconfig",
    url: "https://github.com/neovim/nvim-lspconfig.git",
    commit: "ead0f5f342d8d323441e7d4b88f0fc436a81ad5f",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const POWERSHELL_DOCS: RepoSpec = RepoSpec {
    name: "powershell-docs",
    url: "https://github.com/MicrosoftDocs/PowerShell-Docs.git",
    commit: "b109ee66367572aa731ae8197e73728d3ed0b2ef",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const VITEST: RepoSpec = RepoSpec {
    name: "vitest",
    url: "https://github.com/vitest-dev/vitest",
    commit: "f87817d877788108891c7cc7ca0666531aadb984",
    check_paths: &["."],
    cspell_config: None,
    args: &["--locale=en,en-GB"],
};

pub const FLUTTER_SAMPLES: RepoSpec = RepoSpec {
    name: "flutter-samples",
    url: "https://github.com/flutter/samples",
    commit: "e8557507ffa0a281d6fea4c301e2d5a3cbe0d51a",
    check_paths: &["."],
    cspell_config: None,
    args: &[
        "--locale=en,en-GB,lorem",
        "--exclude=**/*.{pbxproj,xcworkspace,xcworkspacedata,xcscheme,xcconfig,plist}",
    ],
};

pub const DART_SDK: RepoSpec = RepoSpec {
    name: "dart-sdk",
    url: "https://github.com/dart-lang/sdk",
    commit: "6c78e5fed19b193c35aea82beefb1b8af37cbc75",
    check_paths: &["**/*.{dart,md}"],
    cspell_config: None,
    args: &[
        "--locale=en,en-GB",
        "--exclude=tools/dom/**/*.json",
        "--exclude=pkg/*/test/**",
        "--exclude=**/*_data.*",
        "--exclude=runtime/vm/**",
        "--exclude=sdk/lib/html/**",
        "--exclude=benchmarks/**",
        "--exclude=**/*_test.*",
        "--exclude=*/**/*.{json,yaml,yml}",
        "--exclude=tests/corelib/regexp/**",
        "--exclude=**/{third_party,assets}/**",
    ],
};

pub const SLINT: RepoSpec = RepoSpec {
    name: "slint",
    url: "https://github.com/slint-ui/slint",
    commit: "d4483cecc2d4cd6e4e36d9547b89162bd411b907",
    check_paths: &["."],
    cspell_config: None,
    args: &[],
};

/// All repositories for multi-repo integration testing.
/// Matches cspell's integration test suite.
pub const REPOS: &[&RepoSpec] = &[
    &PHP_SRC,
    &SPECBERUS,
    &TYPESCRIPT_STARTER,
    &LICIA,
    &WIRE_WEBAPP,
    &TYPESCRIPT_CHEATSHEETS_REACT,
    &EXPRESS_GRAPHQL,
    &WIRE_DESKTOP,
    &GRAPHQL_RELAY_JS,
    &AWS_AMPLIFY_DOCS,
    &WEBPACK_ASSETS_MANIFEST,
    &PRETTIER,
    &WEBPACK,
    &ARIA_PRACTICES,
    &ADMIN_BRO,
    &TYPESCRIPT_ESLINT,
    &GRAPHQL_JS,
    &TYPESCRIPT_WEBSITE,
    &AZURE_REST_API_SPECS,
    &JIRA,
    &EXONUM,
    &THEALGORITHMS_PYTHON,
    &DJANGO,
    &MEGISTOS,
    &MDX,
    &ADADOOM3,
    &LATEX_EXAMPLES,
    &GOOGLE_CLOUD_CPP,
    &GRAPHQL_SPEC,
    &TPLINK_SMARTHOME_API,
    &OPEN_SOURCE_LOGICIEL_LIBRE,
    &PAGEKIT,
    &BOOTSTRAP,
    &APOLLO_SERVER,
    &SHOELACE,
    &ASPNETBOILERPLATE,
    &CADDY,
    &ESLINT,
    &JAVA_DESIGN_PATTERNS,
    &SQLSERVER_KIT,
    &SVELTE,
    &RUSTPYTHON,
    &CHEF,
    &GITBUCKET,
    &NVIM_LSPCONFIG,
    &POWERSHELL_DOCS,
    &VITEST,
    &FLUTTER_SAMPLES,
    &DART_SDK,
    &SLINT,
];
