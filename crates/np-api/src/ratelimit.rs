//! A small in-memory sliding-window limiter for login, setup and (later) signup, invite and
//! password reset. Limits reset when the process restarts, which is fine for one binary.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use np_core::{Error, ErrorKind};

#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub max: usize,
    pub window: Duration,
}

pub const LOGIN_PER_IP: Limit = Limit {
    max: 20,
    window: Duration::from_secs(5 * 60),
};
pub const LOGIN_PER_EMAIL: Limit = Limit {
    max: 10,
    window: Duration::from_secs(15 * 60),
};
pub const INVITES_PER_USER: Limit = Limit {
    max: 30,
    window: Duration::from_secs(60 * 60),
};
pub const SIGNUP_PER_IP: Limit = Limit {
    max: 10,
    window: Duration::from_secs(60 * 60),
};
pub const RESET_PER_IP: Limit = Limit {
    max: 10,
    window: Duration::from_secs(15 * 60),
};
pub const RESET_PER_EMAIL: Limit = Limit {
    max: 3,
    window: Duration::from_secs(60 * 60),
};
pub const SETUP_PER_IP: Limit = Limit {
    max: 20,
    window: Duration::from_secs(5 * 60),
};

#[derive(Default)]
pub struct RateLimiter {
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RateLimiter {
    /// Records one attempt for `key`; refuses with `rate_limited` once `limit.max` attempts
    /// fall inside the window. Refused attempts are not recorded.
    pub fn check(&self, key: &str, limit: Limit) -> Result<(), Error> {
        self.check_at(key, limit, Instant::now())
    }

    fn check_at(&self, key: &str, limit: Limit, now: Instant) -> Result<(), Error> {
        let mut map = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > 10_000 {
            map.retain(|_, q| {
                q.back()
                    .is_some_and(|t| now.duration_since(*t) < limit.window)
            });
        }
        let q = map.entry(key.to_string()).or_default();
        while q
            .front()
            .is_some_and(|t| now.duration_since(*t) >= limit.window)
        {
            q.pop_front();
        }
        if q.len() >= limit.max {
            let wait = q
                .front()
                .map(|t| limit.window - now.duration_since(*t))
                .unwrap_or_default();
            return Err(Error::new(
                ErrorKind::RateLimited,
                format!(
                    "too many attempts, try again in {} seconds",
                    wait.as_secs().max(1)
                ),
            ));
        }
        q.push_back(now);
        Ok(())
    }

    /// Forgets a key, e.g. after a successful login.
    pub fn reset(&self, key: &str) {
        self.hits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_slides() {
        let rl = RateLimiter::default();
        let lim = Limit {
            max: 2,
            window: Duration::from_secs(10),
        };
        let t0 = Instant::now();
        assert!(rl.check_at("k", lim, t0).is_ok());
        assert!(rl.check_at("k", lim, t0 + Duration::from_secs(1)).is_ok());
        let e = rl
            .check_at("k", lim, t0 + Duration::from_secs(2))
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::RateLimited);
        assert!(rl.check_at("other", lim, t0).is_ok());
        assert!(rl.check_at("k", lim, t0 + Duration::from_secs(11)).is_ok());
        rl.reset("k");
        assert!(rl.check_at("k", lim, t0 + Duration::from_secs(11)).is_ok());
    }
}
