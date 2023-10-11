pub struct NeonRpcClientConfig {
    pub url: String,
}

impl NeonRpcClientConfig {
    pub fn new(url: impl Into<String>) -> NeonRpcClientConfig {
        NeonRpcClientConfig { url: url.into() }
    }
}
