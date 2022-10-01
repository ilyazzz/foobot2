use super::*;
use crate::get_version;
use std::{sync::Arc, time::Instant};
use tokio::fs;

#[derive(Debug, Clone)]
pub struct Ping {
    startup_instant: Arc<Instant>,
}

#[async_trait]
impl ExecutableCommand for Ping {
    fn get_names(&self) -> &[&str] {
        &["ping"]
    }

    fn get_cooldown(&self) -> u64 {
        5
    }

    fn get_permissions(&self) -> Permissions {
        Permissions::Default
    }

    async fn execute<C: ExecutionContext + Send + Sync>(
        &self,
        _ctx: C,
        _trigger_name: &str,
        _args: Vec<&str>,
        _: (&User, &UserIdentifier),
    ) -> Result<Option<String>, CommandError> {
        let uptime = {
            let duration = self.startup_instant.elapsed();

            let minutes = (duration.as_secs() / 60) % 60;
            let hours = (duration.as_secs() / 60) / 60;

            let mut result = String::new();

            if hours != 0 {
                result.push_str(&format!("{}h ", hours));
            };

            if minutes != 0 {
                result.push_str(&format!("{}m ", minutes));
            }

            if result.is_empty() {
                result.push_str(&format!("{}s", duration.as_secs()));
            }

            result
        };

        let smaps = fs::read_to_string("/proc/self/smaps")
            .await
            .expect("Proc FS not found");

        let mut mem_usage = 0; // in KB

        for line in smaps.lines() {
            if line.starts_with("Pss:") || line.starts_with("SwapPss:") {
                let mut split = line.split_whitespace();
                split.next().unwrap();

                let pss = split.next().unwrap();

                mem_usage += pss.parse::<i32>().unwrap();
            }
        }

        Ok(Some(format!(
            "Pong! Version: {}, Uptime {}, RAM usage: {} MiB",
            get_version(),
            uptime,
            mem_usage / 1024,
        )))
    }
}

impl Default for Ping {
    fn default() -> Self {
        Self {
            startup_instant: Arc::new(Instant::now()),
        }
    }
}
