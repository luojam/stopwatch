use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Stopwatch {
    elapsed: Duration,
    started_at: Option<Instant>,
}

impl Stopwatch {
    pub fn start(&mut self) {
        if self.started_at.is_none() {
            self.started_at = Some(Instant::now());
        }
    }

    pub fn pause(&mut self) {
        if let Some(started_at) = self.started_at.take() {
            self.elapsed += started_at.elapsed();
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn is_running(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn elapsed(&self) -> Duration {
        match self.started_at {
            Some(started_at) => self.elapsed + started_at.elapsed(),
            None => self.elapsed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stopwatch_stays_paused_at_zero() {
        let mut timer = Stopwatch::default();

        assert!(!timer.is_running());
        assert_eq!(timer.elapsed(), Duration::ZERO);

        timer.pause();
        assert!(!timer.is_running());
        assert_eq!(timer.elapsed(), Duration::ZERO);
    }

    #[test]
    fn start_pause_and_resume_preserve_accumulated_time() {
        let initial_elapsed = Duration::from_secs(2);
        let mut timer = Stopwatch {
            elapsed: initial_elapsed,
            started_at: None,
        };

        timer.start();
        assert!(timer.is_running());
        let started_at = timer.started_at;
        timer.start();
        assert_eq!(timer.started_at, started_at);
        let running_elapsed = timer.elapsed();
        assert!(running_elapsed >= initial_elapsed);

        timer.pause();
        assert!(!timer.is_running());
        let paused_elapsed = timer.elapsed();
        assert!(paused_elapsed >= running_elapsed);
        timer.pause();
        assert_eq!(timer.elapsed(), paused_elapsed);

        timer.start();
        assert!(timer.is_running());
        assert_eq!(timer.elapsed, paused_elapsed);
        assert!(timer.elapsed() >= paused_elapsed);
    }

    #[test]
    fn reset_clears_running_and_paused_timers_and_allows_restart() {
        for running in [false, true] {
            let mut timer = Stopwatch {
                elapsed: Duration::from_secs(2),
                started_at: None,
            };

            if running {
                timer.start();
            }

            timer.reset();
            assert!(!timer.is_running());
            assert_eq!(timer.elapsed(), Duration::ZERO);

            timer.start();
            assert!(timer.is_running());
            assert_eq!(timer.elapsed, Duration::ZERO);
        }
    }
}
