use crate::error::{KnownErrorsHelper, Result};

const PREFIX: &'static str = "LW";

macro_rules! envname {
    ($name:literal) => {
        format!("{}_{}", PREFIX, $name)
    };
}
macro_rules! parse_env {
    ($name:literal) => {{
        let name = envname!($name);
        let r: Result<String> = Ok(std::env::var(&name).known_error_required(&name)?);
        r
    }};
}
macro_rules! parse_env_opt (
    ($name:literal) => {{
        let name = envname!($name);
        std::env::var(&name).ok()
    }}
);

pub fn indir() -> Result<String> {
    parse_env!("INDIR")
}
pub fn outdir() -> Result<String> {
    parse_env!("OUTDIR")
}
pub fn target_id() -> Result<String> {
    parse_env!("TARGET_ID")
}
pub fn work_name() -> Result<String> {
    parse_env!("WORK_NAME")
}
pub fn work_version() -> Result<String> {
    parse_env!("WORK_VERSION")
}
pub fn mongodb_username() -> Result<String> {
    parse_env!("MONGODB_USERNAME")
}
pub fn mongodb_password() -> Result<String> {
    parse_env!("MONGODB_PASSWORD")
}
pub fn mongodb_host() -> Result<String> {
    parse_env!("MONGODB_HOST")
}
pub fn mongodb_port() -> String {
    parse_env_opt!("MONGODB_PORT").unwrap_or("27017".to_string())
}
pub fn mongodb_options() -> String {
    parse_env_opt!("MONGODB_OPTIONS").unwrap_or("".to_string())
}
pub fn mongodb_database() -> Result<String> {
    parse_env!("MONGODB_DATABASE")
}
pub fn mongodb_collection() -> Result<String> {
    parse_env!("MONGODB_COLLECTION")
}

pub fn s3_access_key() -> Result<String> {
    parse_env!("S3_ACCESS_KEY")
}
pub fn s3_secret_key() -> Result<String> {
    parse_env!("S3_SECRET_KEY")
}
pub fn s3_bucket() -> Result<String> {
    parse_env!("S3_BUCKET")
}
pub fn s3_region_opt() -> Option<String> {
    parse_env_opt!("S3_REGION")
}
pub fn s3_endpoint_opt() -> Option<String> {
    parse_env_opt!("S3_ENDPOINT")
}
pub fn s3_path_style() -> Result<bool> {
    parse_env!("S3_PATH_STYLE").map(|s| s == "true")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Depend {
    pub work_name: String,
    pub work_version: String,
    pub artifacts: Vec<String>,
}

/// parse {PREFIX}_DEPENDS_{WORK_NAME}_{WORK_VERSION} environment variables and return Result<Vec<Depend>>
/// the value of the environments variables is artifacts splited by ';'
///
/// # Examples
///
/// ```
///  use loadwork::envvar;
///  std::env::set_var("LW_DEPENDS_demucs_3", "bass.wav;vocal.wav");
///  let r = envvar::depends().unwrap().into_iter().find(|d| d.work_name == "demucs").unwrap();
///  assert_eq!(r, envvar::Depend {
///     work_name: "demucs".to_string(),
///     work_version: "3".to_string(),
///     artifacts: vec![
///       "bass.wav".to_string(),
///       "vocal.wav".to_string(),
///     ],
///   })
/// ```
pub fn depends() -> Result<Vec<Depend>> {
    let depends = std::env::vars()
        .filter_map(|(k, v)| {
            if v.is_empty() {
                return None;
            }
            k.strip_prefix(&envname!("DEPENDS_")).and_then(|workstr| {
                let (work_name, work_version) = match workstr.split_once('_') {
                    None => (workstr, ""),
                    Some((l, r)) => (l, r),
                };
                if work_name.is_empty() {
                    return None;
                }
                let artifacts = v
                    .split(';')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                Some(Depend {
                    work_name: work_name.to_string(),
                    work_version: work_version.to_string(),
                    artifacts: artifacts,
                })
            })
        })
        .collect();
    Ok(depends)
}
