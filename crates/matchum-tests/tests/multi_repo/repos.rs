use super::harness::RepoSpec;

pub const PHP_SRC: RepoSpec = RepoSpec {
    name: "php-src",
    url: "https://github.com/php/php-src.git",
    commit: "96be28cb0e1869f9c7ddf71d948232a2ab76c63c",
    check_paths: &["**/*.{md,c,h,php}"],
    cspell_config: None,
    args: &[],
};

pub const SPECBERUS: RepoSpec = RepoSpec {
    name: "specberus",
    url: "https://github.com/w3c/specberus.git",
    commit: "25311987a0bb476e68621a71d6104a380fe30db1",
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
    commit: "dd5c2dba10fea73af3ef570cea71b7b426d7ea47",
    check_paths: &["{apps,docs}/**/*.{js,ts,tsx,md,mjs,mts,cjs,cts}"],
    cspell_config: None,
    args: &[],
};

pub const TYPESCRIPT_CHEATSHEETS_REACT: RepoSpec = RepoSpec {
    name: "typescript-cheatsheets-react",
    url: "https://github.com/typescript-cheatsheets/react.git",
    commit: "911f92807caafbf71956973219991aa8e9be1af0",
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
    commit: "072001aea8c52a20bd3c04339ffbc40469af40c0",
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
    commit: "1b21de04c4f6844dfd6af0c4e4c8e051e41d1986",
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
    commit: "f7a1bcfe3344dcc463f24d97da9a5c12d9909e0d",
    check_paths: &[],
    cspell_config: None,
    args: &[],
};

pub const WEBPACK: RepoSpec = RepoSpec {
    name: "webpack",
    url: "https://github.com/webpack/webpack.git",
    commit: "032856c7ba9f4d951e95b111fdabb3447660ce50",
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
    commit: "f645b7f70e11acd953285057b515b20843f69924",
    check_paths: &["**/*.{md,ts,js}"],
    cspell_config: Some(".cspell.json"),
    args: &[],
};

pub const GRAPHQL_JS: RepoSpec = RepoSpec {
    name: "graphql-js",
    url: "https://github.com/graphql/graphql-js.git",
    commit: "41497220bac9b07e8b72f77bbc29655537867f02",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const TYPESCRIPT_WEBSITE: RepoSpec = RepoSpec {
    name: "typescript-website",
    url: "https://github.com/microsoft/TypeScript-Website.git",
    commit: "f31fbb4c3e98ad8fd278fd0ecad47cc1fe4689ea",
    check_paths: &["**/*.*"],
    cspell_config: None,
    args: &[],
};

pub const AZURE_REST_API_SPECS: RepoSpec = RepoSpec {
    name: "azure-rest-api-specs",
    url: "https://github.com/Azure/azure-rest-api-specs.git",
    commit: "1d03fa5b7fa3f86eef71ef4433a474cb14d96443",
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
    commit: "7c54fee7760b1c61fd7f9cb7cc6a2965f4236137",
    check_paths: &["**/*.{md,py}"],
    cspell_config: None,
    args: &["--issues-summary-report"],
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
    commit: "5bb66b79dea59a4b805211b9721e2f237db9acc0",
    check_paths: &[],
    cspell_config: None,
    args: &[
        "**/*",
        "-e",
        "{*.BUILD,BUILD,CHANGELOG.md,*.sh,*.cfg,*.ps1,Dockerfile.*,*.Dockerfile,*.{yaml,xml,json,cmake}}",
    ],
};

pub const GRAPHQL_SPEC: RepoSpec = RepoSpec {
    name: "graphql-spec",
    url: "https://github.com/graphql/graphql-spec.git",
    commit: "61217f05e1d940a85bf9355ed7dc9029bf939335",
    check_paths: &["**/*.md"],
    cspell_config: None,
    args: &["--issues-summary-report"],
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
    commit: "3f69eaece51fad5fd5858cc5847b931db3d2c4a7",
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
        "-e",
        "/app/assets/codemirror",
        "-e",
        "/app/system/languages",
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
    commit: "370727c7bf70d427ad0cbb80d95df226c87dc77a",
    check_paths: &[],
    cspell_config: None,
    args: &["**", "-e", "docs/assets/**"],
};

pub const ASPNETBOILERPLATE: RepoSpec = RepoSpec {
    name: "aspnetboilerplate",
    url: "https://github.com/aspnetboilerplate/aspnetboilerplate",
    commit: "c727b75bfc694cddea1ccb2235720a842df6f58d",
    check_paths: &[],
    cspell_config: None,
    args: &[
        "**/*.{md,cs,cshtml}",
        "--exclude=wwwroot",
        "--exclude=**/*SampleApp.Tests/Web*",
    ],
};

pub const CADDY: RepoSpec = RepoSpec {
    name: "caddy",
    url: "https://github.com/caddyserver/caddy.git",
    commit: "58968b3fd38cacbf4b5e07cc8c8be27696dce60f",
    check_paths: &["**/*.go"],
    cspell_config: None,
    args: &[],
};

pub const ESLINT: RepoSpec = RepoSpec {
    name: "eslint",
    url: "https://github.com/eslint/eslint",
    commit: "6f23076037d5879f20fb3be2ef094293b1e8d38c",
    check_paths: &[],
    cspell_config: None,
    args: &[
        ".",
        "--issues-summary-report",
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
    commit: "a3fcc631672a3e418523af0c61c403caf5c137bf",
    check_paths: &["**/*.md", "**/*.java"],
    cspell_config: None,
    args: &[],
};

pub const SQLSERVER_KIT: RepoSpec = RepoSpec {
    name: "sqlserver-kit",
    url: "https://github.com/ktaranov/sqlserver-kit.git",
    commit: "fa7635fc0a89baedafc01f38bacee5c4cbe214f2",
    check_paths: &[],
    cspell_config: None,
    args: &["--issues-summary-report", "**", "--exclude=**/Backup/**"],
};

pub const SVELTE: RepoSpec = RepoSpec {
    name: "svelte",
    url: "https://github.com/sveltejs/svelte.git",
    commit: "67c4a3811770daeac3ec1df48a99f39659e4a980",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const RUSTPYTHON: RepoSpec = RepoSpec {
    name: "rustpython",
    url: "https://github.com/RustPython/RustPython.git",
    commit: "c974b77127307e24c2c9926f5434498ee1f78df1",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const CHEF: RepoSpec = RepoSpec {
    name: "chef",
    url: "https://github.com/chef/chef.git",
    commit: "c53f3be56b3ea0d9c63af1496d2798357c7dabf5",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const GITBUCKET: RepoSpec = RepoSpec {
    name: "gitbucket",
    url: "https://github.com/gitbucket/gitbucket.git",
    commit: "3826b690cfba07b966c15c4a298171c21f6703d8",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const NVIM_LSPCONFIG: RepoSpec = RepoSpec {
    name: "nvim-lspconfig",
    url: "https://github.com/neovim/nvim-lspconfig.git",
    commit: "66fd02ad1c7ea31616d3ca678fa04e6d0b360824",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const POWERSHELL_DOCS: RepoSpec = RepoSpec {
    name: "powershell-docs",
    url: "https://github.com/MicrosoftDocs/PowerShell-Docs.git",
    commit: "75aaa4850e97816fde550f59af250677f7c9792b",
    check_paths: &["**"],
    cspell_config: None,
    args: &[],
};

pub const VITEST: RepoSpec = RepoSpec {
    name: "vitest",
    url: "https://github.com/vitest-dev/vitest",
    commit: "3425e28fa01559da96ef7e91593b6f16eb05d200",
    check_paths: &["."],
    cspell_config: None,
    args: &["--issues-summary-report", "--locale=en,en-GB"],
};

pub const FLUTTER_SAMPLES: RepoSpec = RepoSpec {
    name: "flutter-samples",
    url: "https://github.com/flutter/samples",
    commit: "54106b0bf6d90bb9ce6fd66e4473be390faae78d",
    check_paths: &["."],
    cspell_config: None,
    args: &[
        "--issues-summary-report",
        "--locale=en,en-GB,lorem",
        "--exclude=**/*.{pbxproj,xcworkspace,xcworkspacedata,xcscheme,xcconfig,plist}",
    ],
};

pub const DART_SDK: RepoSpec = RepoSpec {
    name: "dart-sdk",
    url: "https://github.com/dart-lang/sdk",
    commit: "2076ac5df015b4929317f7a99f1d5430943f8a7c",
    check_paths: &["**/*.{dart,md}"],
    cspell_config: None,
    args: &[
        "--issues-summary-report",
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
    commit: "5ac238182c8877ec0bc1ca6b45468a75eff61355",
    check_paths: &["."],
    cspell_config: None,
    args: &["--issues-summary-report"],
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
