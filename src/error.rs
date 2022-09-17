#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KnownErrors {
    #[error("{0}")]
    Normal(String, bool),
    #[error("{0} is not set")]
    Required(String),
}
impl KnownErrors {
    #[allow(dead_code)]
    pub fn normal<T>(msg: &str, permanent: bool) -> Result<T> {
        Err(Box::new(KnownErrors::Normal(msg.to_string(), permanent)))
    }
    #[allow(dead_code)]
    pub fn required<T>(name: &str) -> Result<T> {
        Err(Box::new(KnownErrors::Required(name.to_string())))
    }
}
pub trait KnownErrorsHelper<T> {
    fn known_error(self, msg: &str, permanent: bool) -> std::result::Result<T, KnownErrors>
    where
        Self: Sized,
    {
        self.known_error_normal(msg, permanent)
    }
    fn known_error_normal(self, msg: &str, permanent: bool) -> std::result::Result<T, KnownErrors>;
    fn known_error_required(self, name: &str) -> std::result::Result<T, KnownErrors>;
}

impl<T, E: std::fmt::Display> KnownErrorsHelper<T> for std::result::Result<T, E> {
    fn known_error_normal(self, msg: &str, permanent: bool) -> std::result::Result<T, KnownErrors> {
        self.map_err(|e| KnownErrors::Normal(format!("{}: {}", msg, e), permanent))
    }
    fn known_error_required(self, name: &str) -> std::result::Result<T, KnownErrors> {
        self.map_err(|e| KnownErrors::Required(format!("{} is required: {}", name.to_string(), e)))
    }
}

impl<T> KnownErrorsHelper<T> for std::io::Error {
    fn known_error_normal(self, msg: &str, permanent: bool) -> std::result::Result<T, KnownErrors> {
        Err(KnownErrors::Normal(
            format!("{}: {:?}", msg, self.kind()),
            permanent,
        ))
    }
    fn known_error_required(self, name: &str) -> std::result::Result<T, KnownErrors> {
        Err(KnownErrors::Required(format!(
            "{} is required: {:?}",
            name.to_string(),
            self.kind()
        )))
    }
}
