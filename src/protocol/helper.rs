use chrono::offset::Utc;
use chrono::DateTime;
use log::{error, info};
use std::time::SystemTime;
use tokio::{sync::mpsc};

use super::{logfile, common::{ListResponse, ListMode, Result, Log, Logs}};

const STORAGE_FILE_PATH: &str = "./log-file.json";

pub async fn get_local_logs() -> Result<Logs>
{
    logfile::read_local_logs(STORAGE_FILE_PATH).await
}

pub async fn create_new_log(content: &str) -> Result<()>
{
    let mut local_logs = logfile::read_local_logs(STORAGE_FILE_PATH).await?;
    let current_timestamp: DateTime<Utc> = SystemTime::now().into();

    local_logs.push(Log {
        timestamp: format!("{}", current_timestamp.format("%d/%m/%Y %T")),
        content: content.to_owned(),
    });

    logfile::write_local_logs(STORAGE_FILE_PATH, &local_logs).await?;

    info!("Created Log:");
    info!("Timestamp: {}", local_logs.last().unwrap().timestamp);
    info!("Content: {}", local_logs.last().unwrap().content);

    Ok(())
}

pub fn respond_with_logs(sender: mpsc::UnboundedSender<ListResponse>, receiver: String)
{
    tokio::spawn(async move {
        match logfile::read_local_logs(STORAGE_FILE_PATH).await
        {
            Ok(logs) =>
            {
                let response = ListResponse {
                    mode: ListMode::All,
                    receiver,
                    data: logs.into_iter().collect(),
                };

                if let Err(error) = sender.send(response)
                {
                    error!("error sending response via channel, {}", error);
                }
            }
            Err(error) =>
            {
                error!(
                    "error fetching local recipes to answer all request, {}",
                    error
                )
            }
        }
    });
}