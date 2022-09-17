use crate::error::{KnownErrors, KnownErrorsHelper, Result};
use async_std::{fs::File, path::Path, stream::StreamExt};
use futures::future::try_join_all;

#[derive(Debug)]
pub struct ConnectorBuilder {
    access_key: String,
    secret_key: String,
    bucket: String,
    region_opt: Option<String>,
    endpoint_opt: Option<String>,
    path_style: bool,
}
impl ConnectorBuilder {
    pub fn new_from_env() -> Result<Self> {
        use crate::envvar;
        let access_key = envvar::s3_access_key()?;
        let secret_key = envvar::s3_secret_key()?;
        let bucket = envvar::s3_bucket()?;
        let region_opt = envvar::s3_region_opt();
        let endpoint_opt = envvar::s3_endpoint_opt();
        let path_style = envvar::s3_path_style()?;
        Ok(Self {
            access_key: access_key,
            secret_key: secret_key,
            bucket: bucket,
            region_opt: region_opt,
            endpoint_opt: endpoint_opt,
            path_style: path_style,
        })
    }
    #[allow(dead_code)]
    pub fn access_key(self, val: &str) -> Self {
        Self {
            access_key: val.to_string(),
            secret_key: self.secret_key,
            bucket: self.bucket,
            region_opt: self.region_opt,
            endpoint_opt: self.endpoint_opt,
            path_style: self.path_style,
        }
    }
    #[allow(dead_code)]
    pub fn secret_key(self, val: &str) -> Self {
        Self {
            access_key: self.access_key,
            secret_key: val.to_string(),
            bucket: self.bucket,
            region_opt: self.region_opt,
            endpoint_opt: self.endpoint_opt,
            path_style: self.path_style,
        }
    }
    #[allow(dead_code)]
    pub fn bucket(self, val: &str) -> Self {
        Self {
            access_key: self.access_key,
            secret_key: self.secret_key,
            bucket: val.to_string(),
            region_opt: self.region_opt,
            endpoint_opt: self.endpoint_opt,
            path_style: self.path_style,
        }
    }
    #[allow(dead_code)]
    pub fn region(self, val: Option<&str>) -> Self {
        Self {
            access_key: self.access_key,
            secret_key: self.secret_key,
            bucket: self.bucket,
            region_opt: val.map(|s| s.to_string()),
            endpoint_opt: self.endpoint_opt,
            path_style: self.path_style,
        }
    }
    #[allow(dead_code)]
    pub fn endpoint(self, val: Option<&str>) -> Self {
        Self {
            access_key: self.access_key,
            secret_key: self.secret_key,
            bucket: self.bucket,
            region_opt: self.region_opt,
            endpoint_opt: val.map(|s| s.to_string()),
            path_style: self.path_style,
        }
    }
    #[allow(dead_code)]
    pub fn path_style(self, val: bool) -> Self {
        Self {
            access_key: self.access_key,
            secret_key: self.secret_key,
            bucket: self.bucket,
            region_opt: self.region_opt,
            endpoint_opt: self.endpoint_opt,
            path_style: val,
        }
    }
    pub fn build(self) -> Result<Connector> {
        Connector::new(
            self.access_key,
            self.secret_key,
            self.bucket,
            self.region_opt,
            self.endpoint_opt,
            self.path_style,
        )
    }
}

#[derive(Debug, Clone)]
pub struct Connector {
    pub endpoint: Option<String>,
    pub region: s3::region::Region,
    pub credentials: s3::creds::Credentials,
    pub bucketname: String,
    pub path_style: bool,
}
impl Connector {
    pub fn new_from_env() -> Result<Self> {
        let b = ConnectorBuilder::new_from_env()?;
        let c = b.build()?;
        Ok(c)
    }
    pub fn new(
        access_key: String,
        secret_key: String,
        bucketname: String,
        region_opt: Option<String>,
        endpoint_opt: Option<String>,
        path_style: bool,
    ) -> Result<Self> {
        let s = Self {
            endpoint: endpoint_opt.clone(),
            region: match (region_opt, endpoint_opt) {
                (_, Some(ep)) => s3::Region::Custom {
                    region: "use-east-1".into(),
                    endpoint: ep,
                },
                (Some(r), _) => {
                    use std::str::FromStr;
                    s3::Region::from_str(&r)?
                }
                _ => Err("").known_error_required("s3_region or s3_endpoint")?,
            },
            credentials: s3::creds::Credentials::new(
                Some(&access_key),
                Some(&secret_key),
                None,
                None,
                None,
            )?,
            bucketname: bucketname,
            path_style: path_style,
        };
        Ok(s)
    }

    #[allow(dead_code)]
    pub fn bucket(&self) -> Result<s3::Bucket> {
        //println!("connect to {:?}", self.endpoint);
        let b = match self.path_style {
            false => s3::Bucket::new(
                &self.bucketname,
                self.region.clone(),
                self.credentials.clone(),
            )
            .known_error("fail to connect bucket", false)?,
            true => s3::Bucket::new_with_path_style(
                &self.bucketname,
                self.region.clone(),
                self.credentials.clone(),
            )
            .known_error("fail to connect bucket", false)?,
        };
        Ok(b)
    }
    #[allow(dead_code)]
    pub async fn create_bucket(
        &self,
        conf: &s3::bucket_ops::BucketConfiguration,
    ) -> Result<s3::bucket_ops::CreateBucketResponse> {
        let r = match self.path_style {
            false => {
                s3::Bucket::create(
                    &self.bucketname,
                    self.region.clone(),
                    self.credentials.clone(),
                    conf.clone(),
                )
                .await?
            }
            true => {
                s3::Bucket::create_with_path_style(
                    &self.bucketname,
                    self.region.clone(),
                    self.credentials.clone(),
                    conf.clone(),
                )
                .await?
            }
        };
        Ok(r)
    }
    #[allow(dead_code)]
    pub async fn download<P: AsRef<Path>>(
        &self,
        target_id: &str,
        depends: &Vec<crate::envvar::Depend>,
        outdir: P,
    ) -> Result<()> {
        let outdir = outdir.as_ref();
        let f_depends = depends.iter().flat_map(|dep| {
            let work_name = dep.work_name.clone();
            dep.artifacts.iter().map(move |artifact| {
                let conn = self.clone();
                let outdir = outdir.clone();
                let work_name = work_name.clone();
                async move {
                    let outpath = outdir.join(work_name.clone()).join(artifact);
                    //let mut io = async_std::fs::File::create(path)
                    //  .await
                    let bucket = conn
                        .bucket()
                        .known_error_normal(&format!("cannot connect to s3"), false)?;
                    let mut outfile = std::fs::File::create(outpath.clone()).known_error_normal(
                        &format!("fail to create file: {}", outpath.display()),
                        false,
                    )?;
                    let s3_path = format!("{}/{}/{}", target_id, work_name, artifact);
                    let code = bucket.get_object_stream(&s3_path, &mut outfile).await?;
                    if code != 200 {
                        return KnownErrors::normal(
                            &format!(
                                "fail to download {} to {}",
                                s3_path,
                                outpath.to_str().unwrap()
                            ),
                            false,
                        );
                    }
                    let _ = outfile.sync_all();
                    /*
                    println!(
                        "download: {} to {}: code={})",
                        s3_path,
                        outpath.to_str().unwrap(),
                        code
                    );
                     */
                    Result::Ok(code)
                } // end of move
            })
        });
        try_join_all(f_depends).await?;
        Ok(())
    }
    #[allow(dead_code)]
    pub async fn upload<P: AsRef<Path>>(
        &self,
        target_id: &str,
        work_name: &str,
        dir: P,
    ) -> Result<Vec<String>> {
        let dir = dir.as_ref();
        let dir_entries = dir
            .read_dir() //Future<Result<ReadDir impl Stream<Result<DirEntry>> >>
            .await?
            // .collect::<Result<Vec<_>, _>>()? //Result<Vec<DirEntry>>? //nightly
            .fold(Ok(Vec::new()), |mut r_acc, r_entry| {
                if r_acc.is_err() {
                    r_acc
                } else if let Err(e) = r_entry {
                    Err(e)
                } else if let Ok(ref mut v) = r_acc {
                    v.push(r_entry.unwrap().clone());
                    r_acc
                } else {
                    r_acc
                }
            })
            .await?;
        //println!("upload files: {:?}", dir_entries);
        let fts = dir_entries.into_iter().map(|e| {
            let conn = self.clone();
            async move {
                let path = e.path();
                let filename = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("Path#file_name")
                    .known_error("fail to get filename", false)?;
                if e.file_type().await?.is_file() {
                    let s3_path = format!("{}/{}/{}", target_id, work_name, filename);
                    /*
                    println!(
                        "upload \"{}\" to \"{}\" ...",
                        path.to_str().unwrap(),
                        s3_path
                    );
                    */
                    let bucket = conn.bucket()?;
                    let mut io = File::open(path.clone())
                        .await
                        .known_error(&format!("fail to open file: {}", path.display()), false)?;
                    let _r = bucket.put_object_stream(&mut io, s3_path).await?;
                    //println!("write status={}", _r);
                    let _r = io.sync_all().await?;
                    Ok(filename.to_string())
                } else {
                    KnownErrors::normal(&format!("invalid output: {}", filename), false)
                }
            }
        });
        let r = try_join_all(fts).await?;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use serial_test::serial;

    #[derive(Debug)]
    struct Setup {
        pub conn: Connector,
        pub indir: String,
        pub outdir: String,
        pub bucketname: String,
        pub bucket: s3::Bucket,
    }
    async fn setup(workname: &str) -> Result<Setup> {
        let indir = async_std::path::Path::new("/tmp/artifact-in");
        let outdir = async_std::path::Path::new("/tmp/artifact-out");
        if indir.exists().await {
            async_std::fs::remove_dir_all(&indir).await?;
        }
        if !indir.exists().await {
            async_std::fs::create_dir(&indir).await?;
        }
        if outdir.exists().await {
            async_std::fs::remove_dir_all(&outdir).await?;
        }
        if !outdir.exists().await {
            async_std::fs::create_dir(&outdir).await?;
        }
        let outdir2 = outdir.join(workname);
        if outdir2.exists().await {
            async_std::fs::remove_dir_all(&outdir2).await?;
        }
        if !outdir2.exists().await {
            async_std::fs::create_dir(&outdir2).await?;
        }

        let bucketname = "testbucket";
        let conn = ConnectorBuilder::new_from_env()?
            .bucket(bucketname)
            .build()?;
        //println!("bucket.url = {}", conn.bucket()?.url());
        let bucket = conn.bucket()?;
        // at first try to delete bucket because to call list against empty bucket raise panic...
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
        /*
                if !r.success() {
                    println!("{}", r.response_text);
                }
        */
        assert!(r.success(), "fail to create bucket: {}", bucketname);
        bucket
            .put_object("/test/up.txt", "Hello World".as_bytes())
            .await?;
        Ok(Setup {
            conn: conn,
            indir: indir.to_str().unwrap().to_string(),
            outdir: outdir.to_str().unwrap().to_string(),
            bucketname: bucketname.to_string(),
            bucket: bucket,
        })
    }

    #[async_std::test]
    #[serial]
    async fn test_artifact_setup() -> Result<()> {
        let _ = setup("test_artifact_setup").await?;
        Ok(())
    }

    #[derive(Debug)]
    struct Upload {
        setup: Setup,
        target_id: String,
        filename: String,
        content: Vec<u8>,
        s3_path: String,
    }
    async fn upload(work_name: &str) -> Result<Upload> {
        let setup = setup(work_name).await?;
        let indir = async_std::path::Path::new(&setup.indir);
        let target_id = "39";
        let filename = format!("{}.txt", work_name);

        let content = "mikumiku".as_bytes();
        let mut file = {
            let f = async_std::fs::File::create(indir.join(filename.clone())).await;
            assert_matches!(f, Ok(_));
            f.unwrap()
        };
        use async_std::io::WriteExt;
        let r = file.write_all(content).await;
        assert_matches!(r, Ok(_));
        let r = file.sync_all().await;
        assert_matches!(r, Ok(_));

        let r = setup.conn.upload(target_id, work_name, indir).await;
        assert_matches!(r, Ok(_));

        Ok(Upload {
            setup: setup,
            target_id: target_id.to_string(),
            filename: filename.clone(),
            content: content.to_vec(),
            s3_path: format!("{}/{}/{}", target_id, work_name, filename),
        })
    }
    #[async_std::test]
    #[serial]
    async fn test_artifact_upload() -> Result<()> {
        let workname = "test_artifact_upload";
        let u = {
            let u = upload(workname).await;
            assert_matches!(u, Ok(_));
            u.unwrap()
        };

        let r = u.setup.bucket.get_object(&u.s3_path).await;
        assert_matches!(r, Ok(_));
        let (data, code) = r.unwrap();
        assert_eq!(code, 200);
        assert_eq!(data, u.content);
        Ok(())
    }

    #[async_std::test]
    #[serial]
    async fn test_artifact_upload_download() -> Result<()> {
        let workname = "test_artifact_upload_download";
        let u = {
            let u = upload(workname).await;
            assert_matches!(u, Ok(_));
            u.unwrap()
        };

        let depend = crate::envvar::Depend {
            work_name: workname.to_string(),
            work_version: "3.9".to_string(),
            artifacts: vec![u.filename.clone()],
        };
        let outdir = async_std::path::Path::new(&u.setup.outdir);
        let r = u
            .setup
            .conn
            .download(&u.target_id, &vec![depend], outdir)
            .await;
        assert_matches!(r, Ok(_));

        let f = async_std::fs::File::open(outdir.join(workname).join(u.filename)).await;
        assert_matches!(f, Ok(_));
        use async_std::io::ReadExt;
        let mut buf = Vec::new();
        let r = f.unwrap().read_to_end(&mut buf).await;
        assert_matches!(r, Ok(_));
        assert_eq!(buf, u.content);
        Ok(())
    }
}
