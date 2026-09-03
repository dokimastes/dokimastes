//! What can be measured from a working tree without running anything:
//! which build systems and languages are present, whether the environment
//! is pinned, whether an ownership map exists. Everything else the
//! qualification asks for is either run (CI) or declared (profile).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Default, Serialize)]
pub struct Measured {
    /// Distinct build systems found, by marker file.
    pub build_systems: BTreeSet<String>,
    /// Source files per language, by extension.
    pub languages: BTreeMap<String, usize>,
    /// Files that pin the environment: container, devbox, toolchain pins.
    pub determinism_markers: Vec<String>,
    /// Where an ownership map was found, if anywhere.
    pub codeowners: Option<String>,
    pub files_scanned: usize,
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "vendor",
    "dist",
    "build",
    ".idea",
    ".gradle",
    "__pycache__",
    ".venv",
    "venv",
];

fn build_system(name: &str) -> Option<&'static str> {
    Some(match name {
        "Cargo.toml" => "cargo",
        "package.json" => "npm",
        "pom.xml" => "maven",
        "build.gradle" | "build.gradle.kts" => "gradle",
        "go.mod" => "go",
        "pyproject.toml" | "setup.py" | "requirements.txt" => "python",
        "Gemfile" => "bundler",
        "composer.json" => "composer",
        "CMakeLists.txt" => "cmake",
        "Makefile" => "make",
        "mix.exs" => "mix",
        "build.sbt" => "sbt",
        _ if name.ends_with(".csproj") || name.ends_with(".sln") => "dotnet",
        _ => return None,
    })
}

fn determinism_marker(name: &str) -> bool {
    matches!(
        name,
        "Dockerfile"
            | "devcontainer.json"
            | "flake.nix"
            | "devbox.json"
            | ".tool-versions"
            | "rust-toolchain.toml"
            | "rust-toolchain"
            | ".nvmrc"
            | ".python-version"
            | ".java-version"
            | "Vagrantfile"
    ) || name.starts_with("Dockerfile.")
}

fn language(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rust",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "go" => "go",
        "py" => "python",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" | "cxx" => "cpp",
        "swift" => "swift",
        "ex" | "exs" => "elixir",
        "sh" | "bash" | "zsh" => "shell",
        "sql" => "sql",
        "tf" => "terraform",
        _ => return None,
    })
}

pub fn measure(root: &Path) -> std::io::Result<Measured> {
    let mut m = Measured::default();
    for codeowners in [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"] {
        if root.join(codeowners).is_file() {
            m.codeowners = Some(codeowners.to_string());
            break;
        }
    }
    walk(root, root, &mut m)?;
    m.determinism_markers.sort();
    m.determinism_markers.dedup();
    Ok(m)
}

fn walk(root: &Path, dir: &Path, m: &mut Measured) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            if name == ".devcontainer" {
                m.determinism_markers.push(".devcontainer/".to_string());
            }
            walk(root, &path, m)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        m.files_scanned += 1;
        if let Some(system) = build_system(&name) {
            m.build_systems.insert(system.to_string());
        }
        if determinism_marker(&name) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            m.determinism_markers.push(rel);
        }
        if let Some(lang) = path.extension().and_then(|e| e.to_str()).and_then(language) {
            *m.languages.entry(lang.to_string()).or_insert(0) += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    #[test]
    fn counts_build_systems_languages_and_markers() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        for f in [
            "Cargo.toml",
            "src/main.rs",
            "src/lib.rs",
            "web/package.json",
            "web/app.ts",
            "Dockerfile",
            ".github/CODEOWNERS",
            "rust-toolchain.toml",
        ] {
            touch(r, f);
        }
        touch(r, "node_modules/x/package.json");
        touch(r, "target/debug/foo.rs");
        touch(r, ".git/HEAD");
        let m = measure(r).unwrap();
        assert_eq!(
            m.build_systems.iter().cloned().collect::<Vec<_>>(),
            vec!["cargo", "npm"]
        );
        assert_eq!(m.languages["rust"], 2);
        assert_eq!(m.languages["typescript"], 1);
        assert_eq!(
            m.determinism_markers,
            vec!["Dockerfile", "rust-toolchain.toml"]
        );
        assert_eq!(m.codeowners.as_deref(), Some(".github/CODEOWNERS"));
    }

    #[test]
    fn an_empty_tree_measures_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let m = measure(dir.path()).unwrap();
        assert!(m.build_systems.is_empty() && m.languages.is_empty() && m.codeowners.is_none());
        assert_eq!(m.files_scanned, 0);
    }
}
