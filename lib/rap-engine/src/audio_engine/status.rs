use std::time::Duration;

#[derive(Clone, Default)]
pub struct EngineStatus {
    pub position: Duration,
    pub is_paused: bool,
    pub volume: f32,
    pub is_empty: bool,
}