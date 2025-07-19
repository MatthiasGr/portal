use tokio::{
    process::Command,
    sync::Mutex,
    task::{self, JoinHandle},
};
use tracing::{Instrument, instrument};

use crate::error::Error;

pub struct ExternalProcess {
    command: String,
    state: Mutex<Option<JoinHandle<()>>>,
}

impl ExternalProcess {
    pub fn new(command: String) -> ExternalProcess {
        ExternalProcess {
            command,
            state: Mutex::new(None),
        }
    }

    #[instrument(skip_all)]
    pub async fn spawn_once(&self) -> Result<bool, Error> {
        let mut lock = self.state.lock().await;
        if let Some(task) = lock.as_mut() {
            if !task.is_finished() {
                tracing::debug!(command = %&self.command, "Previous child process is still running");
                return Ok(false);
            }
            task.await.expect("Panic in external process task");
            tracing::debug!(command = %&self.command, "Previous child process finished");
        }

        let mut process = Command::new(&self.command).kill_on_drop(true).spawn()?;
        tracing::debug!(command = %&self.command, pid = process.id(), "External process created");
        *lock = Some(task::spawn(
            async move {
                match process.wait().await {
                    // TODO: Can we somehow get a reference to self.command in here for tracing?
                    //  Is there a joining join handle that blocks on drop?
                    Ok(status) => {
                        tracing::debug!(status = status.code(), "External process finished")
                    }
                    Err(error) => tracing::debug!(%error, "Error waiting for external process"),
                }
            }
            .in_current_span(),
        ));

        Ok(true)
    }
}

impl Drop for ExternalProcess {
    fn drop(&mut self) {
        if let Some(state) = self.state.get_mut() {
            state.abort();
        }
    }
}
