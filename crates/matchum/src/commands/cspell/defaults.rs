use std::path::Path;

use matchum_config::settings::{CSpellSettings, LanguageSetting, PatternDefinition, StringOrList};

/// Core list of @cspell/cspell-bundled-dicts packages.
/// Each entry is (package_name, primary_dict_name, exact_resolved_version).
///
/// The exact version must match `vendor/cspell/pnpm-lock.yaml`, not the
/// semver range in `packages/cspell-bundled-dicts/package.json`. cspell's
/// integration snapshots are generated from that resolved dependency graph.
pub const DEFAULT_BUNDLED_DICTS: &[(&str, &str, &str)] = &[
    // Always-active global dicts
    ("@cspell/dict-companies", "companies", "3.2.10"),
    ("@cspell/dict-software-terms", "softwareterms", "5.1.20"),
    ("@cspell/dict-public-licenses", "public-licenses", "2.0.15"),
    ("@cspell/dict-filetypes", "filetypes", "3.0.15"),
    ("@cspell/dict-aws", "aws", "4.0.17"),
    ("@cspell/dict-fullstack", "fullstack", "3.2.8"),
    ("@cspell/dict-cryptocurrencies", "cryptocurrencies", "5.0.5"),
    ("@cspell/dict-en_us", "en_us", "4.4.29"),
    (
        "@cspell/dict-en-common-misspellings",
        "en-common-misspellings",
        "2.1.12",
    ),
    ("@cspell/dict-en-gb-mit", "en-gb-mit", "3.1.18"),
    ("@cspell/dict-gaming-terms", "gaming-terms", "1.1.2"),
    ("@cspell/dict-google", "google", "1.0.9"),
    ("@cspell/dict-lorem-ipsum", "lorem-ipsum", "4.0.5"),
    ("@cspell/dict-markdown", "markdown", "2.0.14"),
    // Language-specific dicts (activated via languageSettings per file type)
    ("@cspell/dict-ada", "ada", "4.1.1"),
    ("@cspell/dict-al", "al", "1.1.1"),
    ("@cspell/dict-bash", "bash", "4.2.2"),
    ("@cspell/dict-shell", "shellscript", "1.1.2"),
    ("@cspell/dict-cpp", "cpp", "7.0.2"),
    ("@cspell/dict-csharp", "csharp", "4.0.8"),
    ("@cspell/dict-css", "css", "4.0.19"),
    ("@cspell/dict-dart", "dart", "2.3.2"),
    ("@cspell/dict-data-science", "data-science", "2.0.13"),
    ("@cspell/dict-django", "django", "4.1.6"),
    ("@cspell/dict-docker", "docker", "1.1.17"),
    ("@cspell/dict-dotnet", "dotnet", "5.0.11"),
    ("@cspell/dict-elixir", "elixir", "4.0.8"),
    ("@cspell/dict-flutter", "flutter", "1.1.1"),
    ("@cspell/dict-fonts", "fonts", "4.0.5"),
    ("@cspell/dict-fsharp", "fsharp", "1.1.1"),
    ("@cspell/dict-git", "git", "3.1.0"),
    ("@cspell/dict-golang", "golang", "6.0.26"),
    ("@cspell/dict-haskell", "haskell", "4.0.6"),
    ("@cspell/dict-html", "html", "4.0.14"),
    (
        "@cspell/dict-html-symbol-entities",
        "html-symbol-entities",
        "4.0.5",
    ),
    ("@cspell/dict-java", "java", "5.0.12"),
    ("@cspell/dict-julia", "julia", "1.1.1"),
    ("@cspell/dict-k8s", "k8s", "1.0.12"),
    ("@cspell/dict-kotlin", "kotlin", "1.1.1"),
    ("@cspell/dict-latex", "latex", "5.0.0"),
    ("@cspell/dict-lua", "lua", "4.0.8"),
    ("@cspell/dict-makefile", "makefile", "1.0.5"),
    ("@cspell/dict-monkeyc", "monkeyc", "1.0.12"),
    ("@cspell/dict-node", "node", "5.0.9"),
    ("@cspell/dict-npm", "npm", "5.2.33"),
    ("@cspell/dict-php", "php", "4.1.1"),
    ("@cspell/dict-powershell", "powershell", "5.0.15"),
    ("@cspell/dict-python", "python", "4.2.25"),
    ("@cspell/dict-r", "r", "2.1.1"),
    ("@cspell/dict-ruby", "ruby", "5.1.0"),
    ("@cspell/dict-rust", "rust", "4.1.2"),
    ("@cspell/dict-scala", "scala", "5.0.9"),
    ("@cspell/dict-sql", "sql", "2.2.1"),
    ("@cspell/dict-svelte", "svelte", "1.0.7"),
    ("@cspell/dict-swift", "swift", "2.0.6"),
    ("@cspell/dict-terraform", "terraform", "1.1.3"),
    ("@cspell/dict-typescript", "typescript", "3.2.3"),
    ("@cspell/dict-vue", "vue", "3.0.5"),
    ("@cspell/dict-zig", "zig", "1.0.0"),
];

pub fn bundled_package_version(package_name: &str) -> Option<&'static str> {
    DEFAULT_BUNDLED_DICTS
        .iter()
        .find_map(|(pkg, _, version)| (*pkg == package_name).then_some(*version))
}

/// cspell `predefinedPatterns` from `DefaultSettings.js`.
pub fn predefined_pattern_definitions() -> Vec<PatternDefinition> {
    let pd = |name: &str, pat: &str| PatternDefinition {
        name: name.into(),
        pattern: StringOrList::Single(pat.into()),
    };
    let pdl = |name: &str, pats: &[&str]| PatternDefinition {
        name: name.into(),
        pattern: StringOrList::List(pats.iter().map(|s| (*s).to_string()).collect()),
    };

    vec![
        pd("CommitHash", r"/\b(?![a-f]+\b)(?:0x)?[0-9a-f]{7,}\b/gi"),
        pd("CommitHashLink", r"/\[[0-9a-f]{7,}\]/gi"),
        pd("CStyleHexValue", r"/\b0x[0-9a-f_]+n?\b/gi"),
        pd("CSSHexValue", r"/#[0-9a-f]{3,8}\b/gi"),
        pd("Urls", r#"/(?:https?|ftp):\/\/[^\s"]+/gi"#),
        pd(
            "HexValues",
            r"/(?:#[0-9a-f]{3,8})|(?:0x[0-9a-f]+)|(?:\\u[0-9a-f]{4})|(?:\\x\{[0-9a-f]{4}\})/gi",
        ),
        pdl(
            "SpellCheckerDisable",
            &[
                r"/(\bc?spell(?:-?checker)?::?)\s*disable(?!-line|-next)\b(?!-)[\s\S]*?((?:\1\s*enable\b)|$)/gi",
                r"/^.*\bc?spell(?:-?checker)?::?\s*disable-line\b.*/gim",
                r"/\bc?spell(?:-?checker)?::?\s*disable-next\b.*\s\s?.*/gi",
            ],
        ),
        pd(
            "PublicKey",
            r"/-{5}BEGIN\s+((?:RSA\s+)?PUBLIC\s+KEY)[\w=+\-/=\\\s]+?END\s+\1-{5}/g",
        ),
        pd(
            "RsaCert",
            r"/-{5}BEGIN\s+(CERTIFICATE|(?:RSA\s+)?(?:PRIVATE|PUBLIC)\s+KEY)[\w=+\-/=\\\s]+?END\s+\1-{5}/g",
        ),
        pd(
            "SshRsa",
            r"/ssh-rsa\s+[a-z0-9/+]{28,}={0,3}(?![a-z0-9/+=])/gi",
        ),
        pd("EscapeCharacters", r"/\\(?:[anrvtbf]|[xu][a-f0-9]+)/gi"),
        pd(
            "Base64",
            r"/(?<![A-Za-z0-9/+])(?:[A-Za-z0-9/+]{40,})(?:\s^\s*[A-Za-z0-9/+]{40,})*(?:\s^\s*[A-Za-z0-9/+]+=*)?(?![A-Za-z0-9/+=])/gm",
        ),
        pd(
            "Base64SingleLine",
            r"/(?<=[^A-Za-z0-9/+_]|^)(?=[A-Za-z]{0,80}[0-9+/])(?=[A-Za-z0-9/+]{0,80}?[A-Z][a-z][A-Z])(?=[A-Za-z0-9/+]{0,80}?(?:[A-Z][0-9][A-Z]|[a-z][0-9][a-z]|[A-Z][0-9][a-z]|[a-z][0-9][A-Z]|[0-9][A-Za-z][0-9]))(?=[A-Za-z0-9/+]{0,80}?(?:[a-z]{3}|[A-Z]{3}))(?:[A-Za-z0-9/+]{40,})=*/gm",
        ),
        pd(
            "Base64MultiLine",
            r#"/(?<![A-Za-z0-9/+])["']?(?:[A-Za-z0-9/+]{40,})["']?(?:\s^\s*["']?[A-Za-z0-9/+]{40,}["']?)+(?:\s^\s*["']?[A-Za-z0-9/+]+={0,3}["']?)?(?![A-Za-z0-9/+=])/gm"#,
        ),
        pd(
            "Email",
            r"/<?\b[\w.\-+]{1,128}@\w{1,63}(\.\w{1,63}){1,4}\b>?/gi",
        ),
        pd("SHA", r"/\bsha\d+-[a-z0-9+/]{25,}={0,3}/gi"),
        pd(
            "HashStrings",
            r#"/(?:\b(?:sha\d+|md5|base64|crypt|bcrypt|scrypt|security-token|assertion)[-,:$=]|#code[/])[-\w/+%.]{25,}={0,3}(?:(['"])\s*\+?\s*\1?[-\w/+%.]+={0,3})*(?![-\w/+=%.])/gi"#,
        ),
        pd("UnicodeRef", r"/\bU\+[0-9a-f]{4,5}(?:-[0-9a-f]{4,5})?/gi"),
        pd(
            "UUID",
            r"/\b[0-9a-fx]{8}-[0-9a-fx]{4}-[0-9a-fx]{4}-[0-9a-fx]{4}-[0-9a-fx]{12}\b/gi",
        ),
        pd("href", r#"/\bhref\s*=\s*".*?"/gi"#),
        pd(
            "SpellCheckerDisableBlock",
            r"/(\bc?spell(?:-?checker)?::?)\s*disable(?!-line|-next)\b(?!-)[\s\S]*?((?:\1\s*enable\b)|$)/gi",
        ),
        pd(
            "SpellCheckerDisableLine",
            r"/^.*\bc?spell(?:-?checker)?::?\s*disable-line\b.*/gim",
        ),
        pd(
            "SpellCheckerDisableNext",
            r"/\bc?spell(?:-?checker)?::?\s*disable-next\b.*\s\s?.*/gi",
        ),
        pd(
            "SpellCheckerIgnoreInDocSetting",
            r"/\bc?spell(?:-?checker)?::?\s*(ignoreRegExp|word|ignore).*/gim",
        ),
        pd("PhpHereDoc", r#"/<<<['"]?(\w+)['"]?[\s\S]+?^\1;/gm"#),
        pd(
            "string",
            r#"/(?:(['"]).*?(?<![^\\]\\(\\\\)*)\1)|(?:`[\s\S]*?(?<![^\\]\\(\\\\)*)`)/g"#,
        ),
        pd(
            "CStyleComment",
            r"/(?<!\w:)(?:\/\/.*)|(?:\/\*[\s\S]*?\*\/)/g",
        ),
        pd("Everything", ".*"),
    ]
}

/// Bundled root defaults from `@cspell/cspell-bundled-dicts/cspell-default.config.js`.
///
/// These are merged ahead of user config for `matchum cspell` to match
/// cspell's `getDefaultSettings()` behavior.
pub fn bundled_root_settings() -> CSpellSettings {
    CSpellSettings {
        language: Some("en".into()),
        allow_compound_words: Some(false),
        max_number_of_problems: Some(10_000),
        ..Default::default()
    }
}

/// Core default languageSettings from cspell's `cspell-default.config.js`.
/// These supplement per-package languageSettings and define which dictionaries
/// are activated for each file type.
pub fn default_language_settings() -> Vec<LanguageSetting> {
    let ls = |ids: &[&str], dicts: &[&str]| LanguageSetting {
        language_id: ids.iter().map(|s| s.to_string()).collect(),
        locale: None,
        dictionaries: dicts.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    };
    let pd = |name: &str, pat: &str| PatternDefinition {
        name: name.into(),
        pattern: StringOrList::Single(pat.into()),
    };

    vec![
        ls(
            &["javascript", "javascriptreact"],
            &["typescript", "node", "npm"],
        ),
        ls(
            &["typescript", "typescriptreact", "mdx"],
            &["typescript", "node", "npm"],
        ),
        ls(
            &["javascriptreact", "typescriptreact", "mdx"],
            &["html", "html-symbol-entities", "css", "fonts"],
        ),
        ls(
            &["markdown", "asciidoc"],
            &["npm", "html", "html-symbol-entities"],
        ),
        LanguageSetting {
            language_id: ["html", "pug", "jade", "php", "handlebars"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            locale: None,
            dictionaries: [
                "html",
                "fonts",
                "typescript",
                "css",
                "npm",
                "html-symbol-entities",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            ..Default::default()
        },
        ls(&["json", "jsonc"], &["node", "npm"]),
        ls(&["php"], &["php"]),
        ls(&["css", "less", "scss"], &["fonts", "css"]),
        LanguageSetting {
            language_id: vec!["map".into()],
            enabled: Some(false),
            ..Default::default()
        },
        LanguageSetting {
            language_id: vec!["image".into()],
            enabled: Some(false),
            ..Default::default()
        },
        LanguageSetting {
            language_id: vec!["binary".into()],
            enabled: Some(false),
            ..Default::default()
        },
        LanguageSetting {
            language_id: ["markdown", "html", "mdx"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            locale: None,
            dictionaries: vec![],
            ignore_reg_exp_list: vec!["HTML-symbol-entity".into()],
            patterns: vec![pd("HTML-symbol-entity", r"&[a-z]+;")],
            ..Default::default()
        },
        LanguageSetting {
            language_id: vec!["html".into()],
            locale: None,
            dictionaries: vec![],
            ignore_reg_exp_list: vec!["href".into()],
            patterns: vec![pd("href", r#"\bhref\s*=\s*"[^"]*""#)],
            ..Default::default()
        },
        LanguageSetting {
            language_id: vec!["markdown".into()],
            locale: None,
            dictionaries: vec![],
            ignore_reg_exp_list: vec![
                "MARKDOWN-link-reference".into(),
                "MARKDOWN-link-footer".into(),
                "MARKDOWN-link".into(),
                "MARKDOWN-anchor".into(),
            ],
            patterns: vec![
                pd(
                    "MARKDOWN-link-reference",
                    r#"\]\[[-A-Za-z0-9_.`'"*&;#@ ]+\]"#,
                ),
                pd(
                    "MARKDOWN-link-footer",
                    r#"\[[-A-Za-z0-9_.`'"*&;#@ ]+\]:( [^\s]*)?"#,
                ),
                pd("MARKDOWN-link", r"\]\([^)\s]+"),
                pd("MARKDOWN-anchor", r#"<a\s+id="[^"\s]+"#),
            ],
            ..Default::default()
        },
    ]
}

/// Map a dictionary name (from `dictionaries` array) to an npm package name.
///
/// cspell uses camelCase dict names that map to kebab-case package names,
/// e.g. `softwareTerms` → `@cspell/dict-software-terms`
pub fn dict_name_to_package(dict_name: &str) -> String {
    match dict_name {
        "softwareterms" => return "@cspell/dict-software-terms".into(),
        "en_us" => return "@cspell/dict-en_us".into(),
        "en-gb" => return "@cspell/dict-en-gb-mit".into(),
        "c" => return "@cspell/dict-cpp".into(),
        "public-licenses" => return "@cspell/dict-public-licenses".into(),
        "html-symbol-entities" => return "@cspell/dict-html-symbol-entities".into(),
        "data-science" => return "@cspell/dict-data-science".into(),
        _ => {}
    }
    let mut result = String::from("@cspell/dict-");
    for (i, ch) in dict_name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

/// Map a path to cspell languageIds using the same basename-first behavior as
/// `@cspell/filetypes/findMatchingFileTypes`.
pub fn language_ids_from_path(path: &Path) -> Vec<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let ids: &[&str] = match ext.as_str() {
        "rs" => &["rust"],
        "py" | "pyw" | "pyi" => &["python"],
        "c" | "i" => &["c"],
        "h" => &["cpp"],
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" | "hpp.in" | "h.in" | "ii" | "inl" | "ino"
        | "ipp" | "ixx" | "tpp" | "txx" => &["cpp"],
        "js" | "mjs" | "cjs" | "es6" | "pac" => &["javascript"],
        "ts" | "mts" | "cts" => &["typescript"],
        "jsx" => &["javascriptreact"],
        "tsx" => &["typescriptreact"],
        "java" | "jav" => &["java"],
        "go" => &["go"],
        "rb" | "rake" | "gemspec" | "podspec" | "rbi" | "rbx" | "rjs" | "ru" => &["ruby"],
        "php" | "php4" | "php5" | "phtml" | "ctp" => &["php"],
        "cs" | "csx" | "cake" => &["csharp"],
        "swift" => &["swift"],
        "kt" | "kts" => &["kotlin"],
        "sbt" | "sc" | "scala" => &["scala"],
        "sh" | "bash" | "zsh" => &["shellscript"],
        "containerfile" | "dockerfile" => &["dockerfile"],
        "ps1" | "psm1" | "psd1" | "psrc" | "pssc" => &["powershell"],
        "r" | "rt" => &["r"],
        "lua" => &["lua"],
        "pl" | "pm" | "pod" | "psgi" | "t" => &["perl"],
        "html" | "htm" | "ejs" | "asp" | "aspx" | "jsp" | "jshtm" | "mdoc" | "rhtml" | "shtml"
        | "volt" | "xht" | "xhtml" => &["html"],
        "svelte" => &["svelte"],
        "vue" => &["html", "vue"],
        "astro" => &["astro"],
        "css" => &["css"],
        "scss" => &["scss"],
        "less" => &["less"],
        "json" => &["json"],
        "jsonc" => &["json", "jsonc"],
        "yaml" | "yml" | "cff" | "eyaml" | "eyml" | "yaml-tmlanguage" | "yaml-tmpreferences"
        | "yaml-tmtheme" => &["yaml"],
        "toml" => &["toml"],
        "xml" | "storyboard" | "xib" | "nib" => &["xml"],
        "md" | "markdown" | "mdown" | "mdtext" | "mdtxt" | "mdwn" | "mkd" | "workbook"
        | "markdn" => &["markdown"],
        "mdx" => &["markdown", "mdx"],
        "sql" => &["sql"],
        "txt" => &["plaintext"],
        "dart" => &["dart"],
        "ex" | "exs" => &["elixir"],
        "hs" => &["haskell"],
        "jl" => &["julia"],
        "fs" | "fsx" | "fsi" | "fsscript" => &["fsharp"],
        "zig" => &["zig"],
        "tf" | "tfvars" => &["terraform"],
        "tex" | "latex" | "ltx" | "ctx" => &["latex"],
        "ada" | "adb" | "ads" => &["ada"],
        "proto" | "txtpb" | "textproto" | "textpb" | "pbtxt" => &["protobuf"],
        "graphql" | "gql" => &["graphql"],
        "hbs" | "handlebars" | "hjs" => &["handlebars"],
        "pug" | "jade" => &["jade"],
        "rst" => &["restructuredtext"],
        "adoc" | "asciidoc" | "asc" => &["asciidoc"],
        "vb" => &["vb"],
        _ => {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let name_lower = name.to_lowercase();
            if name_lower == "dockerfile"
                || name_lower == "dockerfile.dev"
                || name_lower.starts_with("dockerfile.")
                || name_lower == "containerfile"
                || name_lower.starts_with("containerfile.")
                || name_lower.ends_with(".dockerfile")
                || name_lower.contains(".dockerfile.")
            {
                return vec!["dockerfile".to_string()];
            }
            let ids: &[&str] = match name_lower.as_str() {
                "makefile" | "gnumakefile" | "ocamlmakefile" => &["makefile"],
                "gemfile" | "appfile" | "appraisals" | "berksfile" | "brewfile" | "capfile"
                | "cheffile" | "dangerfile" | "deliverfile" | "fastfile" | "guardfile"
                | "gymfile" | "hobofile" | "matchfile" | "podfile" | "puppetfile" | "rakefile"
                | "rantfile" => &["ruby"],
                "jakefile" => &["javascript"],
                "readme.md" => &["markdown"],
                _ => &[],
            };
            return ids.iter().map(|id| (*id).to_string()).collect();
        }
    };

    ids.iter().map(|id| (*id).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_BUNDLED_DICTS, bundled_package_version, language_ids_from_path};
    use std::path::Path;

    #[test]
    fn language_ids_match_cspell_scala_extensions() {
        for path in ["build.sbt", "script.sc", "src/main/scala/App.scala"] {
            assert_eq!(language_ids_from_path(Path::new(path)), vec!["scala"]);
        }
    }

    #[test]
    fn language_ids_treat_mdx_as_markdown_and_mdx() {
        assert_eq!(
            language_ids_from_path(Path::new("docs/page.mdx")),
            vec!["markdown", "mdx"]
        );
    }

    #[test]
    fn language_ids_match_cspell_dockerfile_patterns() {
        for path in [
            "Dockerfile",
            "Dockerfile.dev",
            "Dockerfile.torizon-demos",
            "aws.Dockerfile",
            "infra.aws.Dockerfile.prod",
            "Containerfile",
            "Containerfile.arm64",
        ] {
            assert_eq!(language_ids_from_path(Path::new(path)), vec!["dockerfile"]);
        }
    }

    #[test]
    fn bundled_package_versions_match_vendor_lockfile() {
        let lock_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("vendor/cspell/pnpm-lock.yaml");
        let lock = std::fs::read_to_string(&lock_path).expect("read vendor lockfile");

        for &(pkg, _, version) in DEFAULT_BUNDLED_DICTS {
            let needle = format!("'{pkg}':");
            let start = lock.find(&needle).unwrap_or_else(|| {
                panic!("missing {pkg} in {}", lock_path.display());
            });
            let tail = &lock[start..];
            let version_needle = format!("version: {version}");
            assert!(
                tail.contains(&version_needle),
                "expected {pkg} to resolve to {version} in {}",
                lock_path.display()
            );
            assert_eq!(bundled_package_version(pkg), Some(version));
        }
    }
}
