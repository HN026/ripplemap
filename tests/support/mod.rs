use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::TempDir;

pub fn git(path: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args([
            "-c",
            "user.name=Ripplemap test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

pub fn commit(path: &Path) -> anyhow::Result<String> {
    git(path, &["add", "."])?;
    git(path, &["commit", "-m", "fixture"])?;
    git(path, &["rev-parse", "HEAD"])
}

pub fn fixture_repository() -> anyhow::Result<(TempDir, String)> {
    let temporary = tempfile::tempdir()?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in ["package.json", "package-lock.json"] {
        fs::copy(source.join(file), temporary.path().join(file))?;
    }
    for package in ["library", "removed-consumer", "retained-consumer"] {
        let relative = PathBuf::from("fixtures/semantic").join(package);
        fs::create_dir_all(temporary.path().join(&relative).join("src"))?;
        for file in [
            "package.json",
            "tsconfig.json",
            "index.scip",
            "src/index.ts",
        ] {
            fs::copy(
                source.join(&relative).join(file),
                temporary.path().join(&relative).join(file),
            )?;
        }
    }
    git(temporary.path(), &["init", "-b", "main"])?;
    let commit = commit(temporary.path())?;
    Ok((temporary, commit))
}
