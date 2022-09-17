use crate::error::{KnownErrors, KnownErrorsHelper, Result};
use crate::record::{Metadata, WorkRecord, WorkStatus, WorkflowRecord};
use async_std::path::{Path, PathBuf};
use mongodb::bson;

#[derive(Debug, Clone)]
struct Config {
    #[allow(dead_code)]
    indir: String,
    #[allow(dead_code)]
    outdir: String,
    #[allow(dead_code)]
    target_id: String,
    #[allow(dead_code)]
    work_name: String,
    #[allow(dead_code)]
    work_version: String,
    #[allow(dead_code)]
    depends: Vec<crate::envvar::Depend>,
    #[allow(dead_code)]
    record_connector: crate::record::Connector,
    #[allow(dead_code)]
    artifact_connector: crate::artifact::Connector,
}

impl Config {
    pub fn new_from_env() -> Result<Self> {
        use crate::{artifact, envvar, record};
        let s = Self {
            indir: envvar::indir()?,
            outdir: envvar::outdir()?,
            target_id: envvar::target_id()?,
            work_name: envvar::work_name()?,
            work_version: envvar::work_version()?,
            depends: envvar::depends()?,
            artifact_connector: artifact::Connector::new_from_env()?,
            record_connector: record::Connector::new_from_env()?,
        };
        Ok(s)
    }
}

#[allow(dead_code)]
pub async fn run_from_env(args: &[String]) -> Result<()> {
    let config = Config::new_from_env()?;
    run(args, &config).await
}

async fn run(args_: &[String], config: &Config) -> Result<()> {
    if args_.len() < 1 {
        return KnownErrors::normal("program is not given", true);
    };
    let pg = &args_[0];
    let args = &args_[1..];

    let mc = &mut config.record_connector.connect().await?;
    let workflow_record = mc.get_or_default(&config.target_id).await?;

    let result = run_with_record(pg, args, &workflow_record, config).await;
    {
        let work_record = match result {
            Ok((ref metadata, ref uploads)) => WorkRecord {
                name: config.work_name.clone(),
                version: config.work_version.clone(),
                updated: bson::DateTime::from_chrono(chrono::Utc::now()),
                status: WorkStatus::Succeeded,
                error: None,
                metadata: metadata.clone(),
                artifacts: uploads.clone(),
            },
            Err(ref e) => {
                let work_status = match e.downcast_ref::<KnownErrors>() {
                    Some(KnownErrors::Normal(_, false)) => WorkStatus::FailRetryable,
                    _ => WorkStatus::FailPermanent,
                };
                WorkRecord {
                    name: config.work_name.clone(),
                    version: config.work_version.clone(),
                    updated: bson::DateTime::from_chrono(chrono::Utc::now()),
                    status: work_status,
                    error: Some(e.to_string()),
                    metadata: Metadata::new(),
                    artifacts: vec![],
                }
            }
        };
        let _ = mc
            .update_work_record(&config.target_id, &work_record)
            .await?;
    }
    result.and(Ok(()))
}

async fn run_with_record(
    pg: &String,
    args: &[String],
    workflow_record: &WorkflowRecord,
    config: &Config,
) -> Result<(Metadata, Vec<String>)> {
    let dirs = setup_directories(&config.indir, &config.outdir, &config.depends).await?;
    let _depend_records = check_depends(workflow_record, &config.depends).await?;
    let _ = setup_depend_artifacts(
        workflow_record,
        &config.depends,
        &dirs.indir,
        &dirs.indir_artifacts,
        &config.artifact_connector,
        &config.target_id,
    )
    .await?;

    // exec
    let _ = exec(pg, args, &config.target_id, &config.indir, &config.outdir).await?;

    // post-exec
    let uploads = config
        .artifact_connector
        .upload(&config.target_id, &config.work_name, dirs.outdir_artifacts)
        .await?;
    let metadata = crate::record::read_metadata_or_empty(dirs.outdir.join("metadata.json")).await?;
    Ok((metadata, uploads))
}

struct Directories {
    indir: PathBuf,
    indir_artifacts: PathBuf,
    outdir: PathBuf,
    outdir_artifacts: PathBuf,
}
async fn setup_directories(
    indir: &str,
    outdir: &str,
    depends: &Vec<crate::envvar::Depend>,
) -> Result<Directories> {
    let indir = Path::new(&indir);
    let outdir = Path::new(&outdir);
    let indir_artifacts = indir.join("artifacts");
    let outdir_artifacts = outdir.join("artifacts");
    if !indir.exists().await {
        async_std::fs::create_dir(&indir).await.known_error_normal(
            &format!("fail to mkdir: {}", indir.to_str().unwrap()),
            false,
        )?;
        /*
        return KnownErrors::normal(
            &format!("indir {} not found", indir.to_str().unwrap()),
            false,
        );
         */
    }
    if !indir.is_dir().await {
        return KnownErrors::normal(
            &format!("indir {} is not a directory", indir.to_str().unwrap()),
            false,
        );
    }
    if !indir_artifacts.exists().await {
        async_std::fs::create_dir(&indir_artifacts)
            .await
            .known_error_normal(
                &format!("fail to mkdir: {}", indir_artifacts.to_str().unwrap()),
                false,
            )?;
    } else if !indir_artifacts.is_dir().await {
        return KnownErrors::normal(
            &format!(
                "indir {} is not a directory",
                indir_artifacts.to_str().unwrap()
            ),
            false,
        );
    }
    for dep in depends.iter() {
        let d = indir_artifacts.join(&dep.work_name);
        if !d.exists().await {
            async_std::fs::create_dir(&d)
                .await
                .known_error_normal(&format!("fail to mkdir: {}", d.to_str().unwrap()), false)?;
        } else if !d.is_dir().await {
            return KnownErrors::normal(
                &format!("indir {} is not a directory", d.to_str().unwrap()),
                false,
            );
        }
    }

    if !outdir.exists().await {
        async_std::fs::create_dir(&outdir)
            .await
            .known_error_normal(
                &format!("fail to mkdir: {}", outdir.to_str().unwrap()),
                false,
            )?;
        /*
        return KnownErrors::normal(
            &format!("outdir {} not found", outdir.to_str().unwrap()),
            false,
        );
        */
    }
    if !outdir.is_dir().await {
        return KnownErrors::normal(
            &format!("outdir {} is not a directory", outdir.to_str().unwrap()),
            false,
        );
    }
    if !outdir_artifacts.exists().await {
        async_std::fs::create_dir(&outdir_artifacts)
            .await
            .known_error_normal(
                &format!(
                    "fail to mkdir: {}",
                    outdir_artifacts.to_str().unwrap().to_string()
                ),
                false,
            )?;
    } else if !outdir_artifacts.is_dir().await {
        return KnownErrors::normal(
            &format!(
                "indir {} is not a directory",
                outdir_artifacts.to_str().unwrap().to_string()
            ),
            false,
        );
    }
    Ok(Directories {
        indir: indir.to_path_buf(),
        indir_artifacts: indir_artifacts.to_path_buf(),
        outdir: outdir.to_path_buf(),
        outdir_artifacts: outdir_artifacts.to_path_buf(),
    })
}

async fn check_depends(
    workflow_record: &WorkflowRecord,
    depends: &Vec<crate::envvar::Depend>,
) -> Result<Vec<WorkRecord>> {
    let rets = depends.iter().fold(Ok(Vec::new()), |acc, dep| {
        acc.and_then(
            |mut acc_vec| match workflow_record.works.get(&dep.work_name) {
                None => KnownErrors::normal(
                    &format!("Work '{}' is not completed yet", dep.work_name),
                    false,
                ),
                Some(w) if (w.version != dep.work_version) => KnownErrors::normal(
                    &format!(
                        "Work '{}' version mismatched: {} but {}",
                        dep.work_name, dep.work_version, w.version,
                    ),
                    false,
                ),
                Some(w) => {
                    acc_vec.push(w.clone());
                    Ok(acc_vec)
                }
            },
        )
    })?;
    Ok(rets)
}

async fn setup_depend_artifacts(
    workflow_record: &WorkflowRecord,
    depends: &Vec<crate::envvar::Depend>,
    indir: &Path,
    indir_artifact: &Path,
    artifact_connector: &crate::artifact::Connector,
    target_id: &str,
) -> Result<()> {
    let _ =
        crate::record::write_workflow_record(workflow_record, indir.join("workflow.json")).await?;
    let _ = artifact_connector
        .download(target_id, depends, indir_artifact)
        .await?;
    Ok(())
}

async fn exec(
    pg: &String,
    args: &[String],
    target_id: &str,
    indir: &str,
    outdir: &str,
) -> Result<()> {
    let r = std::process::Command::new(pg)
        .args(args)
        .env_clear()
        .env("LW_TARGET_ID", target_id)
        .env("LW_INDIR", indir)
        .env("LW_OUTDIR", outdir)
        .status();

    use std::os::unix::process::ExitStatusExt;
    match r {
        Err(e) => Err(e).known_error(&format!("{}: fail to exec", pg), false),
        Ok(exit_status) => match (exit_status.signal(), exit_status.code()) {
            (Some(sig), _) => {
                Err(format!("killed by {}", sig)).known_error(&format!("{}", pg), true)
            }
            (None, Some(0)) => Ok(()),
            (None, Some(code)) => {
                //i32
                let retryable = 0 < code;
                Err(format!("exits with {}", code)).known_error(&format!("{}", pg), retryable)
            }
            (None, None) => {
                Err(format!("no status and signal")).known_error(&format!("{}", pg), false)
            }
        },
    }?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::record::{doc, WorkRecord, WorkStatus};
    use assert_matches::assert_matches;
    use serial_test::serial;

    struct Setup {
        pub config: Config,
    }
    async fn clear_directory(config: &Config) -> Result<()> {
        let indir = async_std::path::Path::new(&config.indir);
        let outdir = async_std::path::Path::new(&config.outdir);
        if indir.exists().await {
            async_std::fs::remove_dir_all(&indir).await?;
        }
        if outdir.exists().await {
            async_std::fs::remove_dir_all(&outdir).await?;
        }
        Ok(())
    }
    async fn setup() -> Result<Setup> {
        let config = Config::new_from_env()?;
        clear_directory(&config).await?;
        {
            let conn = &config.artifact_connector;
            let bucket = conn.bucket()?;
            let r = bucket.delete().await.unwrap_or(0u16);
            match r {
                409 => {
                    // conflicts. it occures when bucket has any objects.
                    for r in bucket.list("/".to_string(), None).await?.iter() {
                        //println!("list item: {:?}", r);
                        for c in r.contents.iter() {
                            //println!("delete object: {}", c.key);
                            let (_, _r) = bucket.delete_object(&c.key).await?;
                            //println!("  code={}", r); //204
                        }
                    }
                    let r = bucket.delete().await.unwrap_or(0u16);
                    assert_eq!(r, 204);
                }
                204 => (),
                404 => (),
                _ => {
                    assert!(false, "fail to delete bucket: {}", r);
                }
            }
            let conf = s3::bucket_ops::BucketConfiguration::public();
            let r = conn.create_bucket(&conf).await?;
            assert!(r.success(), "fail to create bucket: {}", conn.bucketname);
        }

        Ok(Setup { config: config })
    }

    #[async_std::test]
    #[serial]
    async fn test_run_malformed_json() -> Result<()> {
        let setup = setup().await?;
        let mut mc = setup.config.record_connector.connect().await?;
        assert_matches!(mc.delete_all().await, Ok(()));

        assert_matches!(mc.get_or_default(&setup.config.target_id).await, Ok(_));

        let metadata = doc! { "hello": "world" };
        assert_matches!(
            mc.update_work_record(
                &setup.config.target_id,
                &WorkRecord {
                    name: setup.config.work_name.clone(),
                    version: setup.config.work_version.clone(),
                    status: WorkStatus::NotStarted,
                    error: None,
                    updated: bson::DateTime::from(chrono::Utc::now()),
                    metadata: metadata.clone(),
                    artifacts: vec![],
                }
            )
            .await,
            Ok(())
        );

        let args = vec![
            "/bin/bash".to_string(),
            "-c".to_string(),
            "echo '{\"hello\",\"universe\"}' > $LW_OUTDIR/metadata.json".to_string(),
        ];
        let r = run(&args, &setup.config).await;
        assert!(r.is_err());
        match r.err().unwrap().downcast_ref::<KnownErrors>() {
            None => assert!(false),
            Some(e) => assert!(
                e.to_string().contains("malformed json"),
                "mismatch: {}",
                e.to_string()
            ),
        };
        Ok(())
    }

    #[async_std::test]
    #[serial]
    async fn test_run_result() -> Result<()> {
        let setup = setup().await?;
        let mut mc = setup.config.record_connector.connect().await?;
        assert_matches!(mc.delete_all().await, Ok(()));

        let args = vec![
            "/bin/bash".to_string(),
            "-c".to_string(),
            "touch $LW_OUTDIR/artifacts/excaliver; echo '{\"hello\":\"universe\"}' > $LW_OUTDIR/metadata.json; ".to_string(),
        ];
        let r = run(&args, &setup.config).await;
        assert_matches!(r, Ok(()));

        let workflow2 = mc.get_or_default(&setup.config.target_id).await.unwrap();
        assert_eq!(workflow2.id, setup.config.target_id);
        assert_eq!(workflow2.works.len(), 1);
        let work2 = &workflow2.works.get(&setup.config.work_name);
        assert_matches!(work2, Some(_));
        let work2 = work2.unwrap();
        assert_eq!(work2.name, setup.config.work_name);
        let md = work2.metadata.get_str("hello");
        assert_matches!(md, Ok(_));
        assert_eq!(md.unwrap(), "universe");
        assert_eq!(work2.artifacts.len(), 1);
        assert_eq!(work2.artifacts[0], "excaliver");

        Ok(())
    }

    #[async_std::test]
    #[serial]
    async fn test_run_depend_fail() -> Result<()> {
        let depend = crate::envvar::Depend {
            work_name: "depend".to_string(),
            work_version: "tmp".to_string(),
            artifacts: vec![],
        };
        let mut setup = setup().await?;

        let mut mc = setup.config.record_connector.connect().await?;
        assert_matches!(mc.delete_all().await, Ok(()));

        let args = vec![
            "/bin/bash".to_string(),
            "-c".to_string(),
            "echo '{\"hello\":\"universe\"}' > $LW_OUTDIR/metadata.json".to_string(),
        ];
        setup.config.depends = vec![depend];
        let r = run(&args, &setup.config).await;
        assert_matches!(r, Err(_));
        assert_eq!(
            r.err().unwrap().downcast_ref::<KnownErrors>(),
            Some(&KnownErrors::Normal(
                "Work 'depend' is not completed yet".to_string(),
                false
            ))
        );

        let workflow2 = mc.get_or_default(&setup.config.target_id).await.unwrap();
        assert_eq!(workflow2.works.len(), 1);
        let work2 = &workflow2.works.get(&setup.config.work_name);
        assert_matches!(work2, Some(_));
        let work2 = work2.unwrap();
        assert_eq!(work2.name, setup.config.work_name);
        assert_matches!(work2.status, WorkStatus::FailRetryable);

        Ok(())
    }

    #[async_std::test]
    #[serial]
    async fn test_run_depend_version_fail() -> Result<()> {
        let depend = crate::envvar::Depend {
            work_name: "depend".to_string(),
            work_version: "tmp".to_string(),
            artifacts: vec!["excaliver".to_string()],
        };
        let mut setup = setup().await?;

        let mut mc = setup.config.record_connector.connect().await?;
        assert_matches!(mc.delete_all().await, Ok(()));

        {
            let args = vec![
                "/bin/bash".to_string(),
                "-c".to_string(),
                "echo '{\"hello\":\"depend\"}' > $LW_OUTDIR/metadata.json".to_string(),
            ];
            let mut config = setup.config.clone();
            config.work_name = depend.work_name.clone();
            config.work_version = "mismatched-version".to_string();
            let r = run(&args, &config).await;
            assert_matches!(r, Ok(_));

            let workflow2 = mc.get_or_default(&setup.config.target_id).await.unwrap();
            assert_eq!(workflow2.works.len(), 1);
            let work2 = &workflow2.works.get(&config.work_name);
            assert_matches!(work2, Some(_));
            let work2 = work2.unwrap();
            assert_eq!(work2.name, config.work_name);
            assert_matches!(work2.status, WorkStatus::Succeeded);
            clear_directory(&config).await?;
        }

        let args = vec![
            "/bin/bash".to_string(),
            "-c".to_string(),
            "echo '{\"hello\":\"universe\"}' > $LW_OUTDIR/metadata.json".to_string(),
        ];
        setup.config.depends = vec![depend.clone()];
        let r = run(&args, &setup.config).await;
        assert_matches!(r, Err(_));
        assert_eq!(
            r.err().unwrap().downcast_ref::<KnownErrors>(),
            Some(&KnownErrors::Normal(
                "Work 'depend' version mismatched: tmp but mismatched-version".to_string(),
                false
            ))
        );

        let workflow2 = mc.get_or_default(&setup.config.target_id).await.unwrap();
        assert_eq!(workflow2.works.len(), 2);
        let work2 = &workflow2.works.get(&setup.config.work_name);
        assert_matches!(work2, Some(_));
        let work2 = work2.unwrap();
        assert_matches!(work2.status, WorkStatus::FailRetryable);

        Ok(())
    }

    #[async_std::test]
    #[serial]
    async fn test_run_depend_artifact_fail() -> Result<()> {
        let depend = crate::envvar::Depend {
            work_name: "depend".to_string(),
            work_version: "tmp".to_string(),
            artifacts: vec!["excaliver".to_string()],
        };
        let mut setup = setup().await?;

        let mut mc = setup.config.record_connector.connect().await?;
        assert_matches!(mc.delete_all().await, Ok(()));

        {
            let args = vec![
                "/bin/bash".to_string(),
                "-c".to_string(),
                "echo '{\"hello\":\"depend\"}' > $LW_OUTDIR/metadata.json".to_string(),
            ];
            let mut config = setup.config.clone();
            config.work_name = depend.work_name.clone();
            config.work_version = depend.work_version.clone();
            let r = run(&args, &config).await;
            assert_matches!(r, Ok(_));

            let workflow2 = mc.get_or_default(&setup.config.target_id).await.unwrap();
            assert_eq!(workflow2.works.len(), 1);
            let work2 = &workflow2.works.get(&config.work_name);
            assert_matches!(work2, Some(_));
            let work2 = work2.unwrap();
            assert_eq!(work2.name, config.work_name);
            assert_matches!(work2.status, WorkStatus::Succeeded);
            clear_directory(&config).await?;
        }

        let args = vec![
            "/bin/bash".to_string(),
            "-c".to_string(),
            "echo '{\"hello\":\"universe\"}' > $LW_OUTDIR/metadata.json".to_string(),
        ];
        setup.config.depends = vec![depend.clone()];
        let r = run(&args, &setup.config).await;
        assert_matches!(r, Err(_));
        assert_eq!(
            r.err().unwrap().downcast_ref::<KnownErrors>(),
            Some(&KnownErrors::Normal(
                format!(
                    "fail to download {}/{}/{} to {}/artifacts/{}/{}",
                    setup.config.target_id,
                    depend.work_name,
                    depend.artifacts[0].clone(),
                    setup.config.indir,
                    depend.work_name,
                    depend.artifacts[0].clone()
                ),
                false
            ))
        );

        let workflow2 = mc.get_or_default(&setup.config.target_id).await.unwrap();
        assert_eq!(workflow2.works.len(), 2);
        let work2 = &workflow2.works.get(&setup.config.work_name);
        assert_matches!(work2, Some(_));
        let work2 = work2.unwrap();
        assert_eq!(work2.name, setup.config.work_name);
        assert_matches!(work2.status, WorkStatus::FailRetryable);

        Ok(())
    }

    #[async_std::test]
    #[serial]
    async fn test_run_depend_artifact_success() -> Result<()> {
        let depend = crate::envvar::Depend {
            work_name: "depend".to_string(),
            work_version: "tmp".to_string(),
            artifacts: vec!["excaliver".to_string()],
        };
        let mut setup = setup().await?;

        let mut mc = setup.config.record_connector.connect().await?;
        assert_matches!(mc.delete_all().await, Ok(()));

        {
            let args = vec![
                "/bin/bash".to_string(),
                "-c".to_string(),
                "echo 'ex' > $LW_OUTDIR/artifacts/excaliver;  echo '{\"hello\":\"depend\"}' > $LW_OUTDIR/metadata.json".to_string(),
            ];
            let mut config = setup.config.clone();
            config.work_name = depend.work_name.clone();
            config.work_version = depend.work_version.clone();
            let r = run(&args, &config).await;
            assert_matches!(r, Ok(_));

            let workflow2 = mc.get_or_default(&setup.config.target_id).await.unwrap();
            assert_eq!(workflow2.works.len(), 1);
            let work2 = &workflow2.works.get(&config.work_name);
            assert_matches!(work2, Some(_));
            let work2 = work2.unwrap();
            assert_eq!(work2.name, config.work_name);
            assert_matches!(work2.status, WorkStatus::Succeeded);
            clear_directory(&config).await?;
        }

        let args = vec![
            "/bin/bash".to_string(),
            "-c".to_string(),
            "echo '{\"hello\":\"universe\"}' > $LW_OUTDIR/metadata.json".to_string(),
        ];
        setup.config.depends = vec![depend.clone()];
        let r = run(&args, &setup.config).await;
        assert_matches!(r, Ok(_));

        let workflow2 = mc.get_or_default(&setup.config.target_id).await.unwrap();
        assert_eq!(workflow2.works.len(), 2);
        let work2 = &workflow2.works.get(&setup.config.work_name);
        assert_matches!(work2, Some(_));
        let work2 = work2.unwrap();
        assert_eq!(work2.name, setup.config.work_name);
        assert_matches!(work2.status, WorkStatus::Succeeded);

        Ok(())
    }
}
