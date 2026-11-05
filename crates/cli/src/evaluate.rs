//! Shared Harbor launcher: one development attempt, then isolated evaluation sessions.
use anyhow::{Context, Result, ensure};
use clap::Args;
use std::{path::PathBuf, process::Command};
#[derive(Args)]
pub struct Evaluate {
    /// Task name under bench/tasks, or a task directory.
    #[arg(long)]
    pub task: PathBuf,
    /// Frozen release directory (required for atomic-arb).
    #[arg(long)]
    pub release: Option<PathBuf>,
    #[arg(long, default_value = "codex")]
    pub agent: String,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long, default_value = "high")]
    pub effort: String,
    #[arg(long)]
    pub job_name: String,
    #[arg(long, default_value = "jobs")]
    pub jobs_dir: PathBuf,
    /// Validate inputs and print the command without running the agent.
    #[arg(long)]
    pub dry_run: bool,
    /// Reuse completed sessions from this same scenario-set job.
    #[arg(long)]
    pub resume: bool,
    /// Explicitly retry only an infrastructure-failed session with no score.
    #[arg(long, requires = "resume")]
    pub retry_infrastructure: bool,
}
pub fn run(args: Evaluate) -> Result<String> {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let task = if let Some(release) = &args.release {
        ensure!(
            !release.join("FREEZE_FAILED").exists(),
            "release failed acceptance"
        );
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(release.join("release.json"))?)?;
        if metadata["kind"] == "private-scenario-set" {
            ensure!(
                args.task.file_name().is_some_and(|n| n == "atomic-arb"),
                "release belongs to atomic-arb"
            );
            let mut command = Command::new("python3");
            command
                .arg(repo.join("bench/backrun/evaluate_set.py"))
                .arg("--release")
                .arg(release)
                .arg("--jobs-dir")
                .arg(&args.jobs_dir)
                .args([
                    "--job-name",
                    &args.job_name,
                    "--agent",
                    &args.agent,
                    "--effort",
                    &args.effort,
                ]);
            if let Some(model) = &args.model {
                command.args(["--model", model]);
            }
            if args.dry_run {
                command.arg("--dry-run");
            }
            if args.resume {
                command.arg("--resume");
            }
            if args.retry_infrastructure {
                command.arg("--retry-infrastructure");
            }
            ensure!(
                command.status()?.success(),
                "scenario-set evaluation failed; inspect the retained report and logs"
            );
            return Ok(serde_json::json!({"report":args.jobs_dir.join(&args.job_name).join("report.json"),"aggregation":null,"dry_run":args.dry_run}).to_string());
        }
        ensure!(
            !args.resume && !args.retry_infrastructure,
            "recovery options require a scenario-set release"
        );
        ensure!(
            metadata["protocol"] == "continuous-arrival-v1",
            "use a validated continuous-arrival release"
        );
        ensure!(
            args.task.file_name().is_some_and(|n| n == "atomic-arb"),
            "release belongs to atomic-arb"
        );
        for image in metadata["images"]
            .as_object()
            .context("missing pinned images")?
            .values()
        {
            ensure!(
                Command::new("docker")
                    .args(["image", "inspect", image.as_str().context("invalid image")?])
                    .stdout(std::process::Stdio::null())
                    .status()?
                    .success(),
                "missing frozen image"
            );
        }
        release.join("task")
    } else {
        ensure!(
            args.task.file_name().is_none_or(|n| n != "atomic-arb"),
            "atomic-arb requires --release with completed acceptance evidence"
        );
        if args.task.is_dir() {
            args.task.clone()
        } else {
            repo.join("bench/tasks").join(&args.task)
        }
    };
    ensure!(task.join("task.toml").is_file(), "task.toml missing");
    ensure!(
        !args.jobs_dir.join(&args.job_name).exists(),
        "job already exists; retain previous results and use a new name"
    );
    let mut command = Command::new("harbor");
    command
        .args(["run", "--path"])
        .arg(task.canonicalize()?)
        .args([
            "--agent",
            &args.agent,
            "--effort",
            &args.effort,
            "--n-concurrent",
            "1",
            "--n-attempts",
            "1",
            "--max-retries",
            "0",
            "--jobs-dir",
        ])
        .arg(&args.jobs_dir)
        .args(["--job-name", &args.job_name]);
    if let Some(model) = args.model {
        command.args(["--model", &model]);
    }
    if args.agent == "codex" {
        let auth = std::env::var_os("CODEX_AUTH_JSON_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".codex/auth.json")
            });
        ensure!(
            auth.is_file(),
            "Codex auth missing: run codex login or set CODEX_AUTH_JSON_PATH"
        );
        command.env("CODEX_AUTH_JSON_PATH", auth).args([
            "--agent-kwarg",
            "version=0.166.0",
            "--agent-kwarg",
            "config={}",
        ]);
    }
    if args.dry_run {
        return Ok(format!("{command:?}"));
    }
    ensure!(
        command.status()?.success(),
        "Harbor evaluation failed; inspect the retained job logs"
    );
    Ok(serde_json::json!({"job":args.jobs_dir.join(args.job_name)}).to_string())
}
