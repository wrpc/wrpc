
use anyhow::{ensure, Context};
use tokio::process::Command;

mod common;
use common::{init};

#[tokio::test(flavor = "multi_thread")]
async fn go() -> anyhow::Result<()> {
    init().await;

    let status = Command::new("go")
        .current_dir("go")
        .args(["test", "-v", "./..."])
        .kill_on_drop(true)
        .status()
        .await
        .context("failed to call `go test`")?;
    ensure!(status.success(), "`go test` failed");
    Ok(())
}
