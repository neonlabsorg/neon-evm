use crate::Config;
use neon_lib::types::TracerDb;
use std::sync::Arc;

#[derive(Clone)]
pub struct State {
    pub config: Arc<Config>,
    pub tracer_db: TracerDb,
}

impl State {
    pub fn new(config: Config) -> Self {
        Self {
            tracer_db: TracerDb::new(config.db_config.as_ref().expect("db-config not found")),
            config: Arc::new(config),
        }
    }
}
