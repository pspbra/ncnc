use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};

/// 请求开始时消耗额度；无需后台补充任务，取消等待也不会占用额度。
pub struct RateLimiter {
    qps: f64,
    capacity: f64,
    state: Mutex<State>,
}

struct State {
    tokens: f64,
    updated_at: Instant,
}

impl RateLimiter {
    pub fn new(qps: u64, burst: usize) -> Self {
        assert!(qps > 0 && burst > 0);
        Self {
            qps: qps as f64,
            capacity: burst as f64,
            state: Mutex::new(State {
                tokens: burst as f64,
                updated_at: Instant::now(),
            }),
        }
    }

    pub async fn acquire(&self) {
        loop {
            let delay = {
                let mut state = self.state.lock().await;
                let now = Instant::now();
                state.tokens = (state.tokens
                    + now.duration_since(state.updated_at).as_secs_f64() * self.qps)
                    .min(self.capacity);
                state.updated_at = now;
                if state.tokens >= 1.0 {
                    state.tokens -= 1.0;
                    return;
                }
                Duration::from_secs_f64((1.0 - state.tokens) / self.qps)
            };
            sleep(delay).await;
        }
    }
}
