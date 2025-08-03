use std::sync::Mutex;
use std::time::Duration;

use names::Generator;

/// Internal engine state, shared across requests.
pub struct InnerEngine {
    /// Name generator for IRC nicknames.
    nicknames: Mutex<Generator<'static>>,
    /// Timeout duration for IRC responses.
    pub timeout: Duration,
    /// Username generator for IRC usernames.
    usernames: Option<Mutex<Generator<'static>>>,
}

impl Default for InnerEngine {
    fn default() -> Self {
        Self {
            nicknames: Default::default(),
            timeout: Duration::from_secs(30),
            usernames: Default::default(),
        }
    }
}

impl std::fmt::Debug for InnerEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(InnerEngine))
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl InnerEngine {
    /// Generate the next unique IRC nickname.
    pub fn next_nickname(&self) -> Option<String> {
        if let Ok(mut lock) = self.nicknames.lock() {
            lock.next()
        } else {
            None
        }
    }

    /// Generate the next unique IRC username.
    pub fn next_username(&self) -> Option<String> {
        if let Some(usernames) = &self.usernames {
            if let Ok(mut lock) = usernames.lock() {
                return lock.next();
            }
        }
        None
    }
}
