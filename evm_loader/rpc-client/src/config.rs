pub struct NeonRpcClientConfig {
    pub url: String,
}

impl NeonRpcClientConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}
