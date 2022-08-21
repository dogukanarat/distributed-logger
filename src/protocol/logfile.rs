use tokio::{fs};

use super::common::{Logs, Result};

pub async fn read_local_logs(file_path: &str) -> Result<Logs>
{
    let content = fs::read(file_path).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}

pub async fn write_local_logs(file_path: &str, logs: &Logs) -> Result<()>
{
    let json = serde_json::to_string(&logs)?;
    fs::write(file_path, &json).await?;
    Ok(())
}
