use crate::error::{KnownErrors, KnownErrorsHelper, Result};
use std::collections::HashMap;

use async_std::{
    fs::File,
    io::{Read, ReadExt, Write, WriteExt},
    path::Path,
};
//use chrono;
pub use mongodb::bson::{doc, Document};
use mongodb::{bson, options::UpdateOptions};
use serde;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum WorkStatus {
    NotStarted = 0,
    Succeeded = 1,
    FailRetryable = 10,
    FailPermanent = 11,
}

pub type Metadata = Document;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WorkRecord {
    pub name: String,
    pub version: String,
    pub status: WorkStatus,
    pub error: Option<String>,
    //pub updated: chrono::NaiveDateTime,
    pub updated: bson::DateTime,
    pub artifacts: Vec<String>,
    pub metadata: Metadata,
}

pub type WorkRecordMap = HashMap<String, WorkRecord>;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WorkflowRecord {
    pub id: String,
    pub works: WorkRecordMap,
}

fn db_key(target_id: &str) -> Document {
    doc! { "id": target_id.to_string() }
}

#[derive(Debug, Clone)]
pub struct Connector {
    urlbase: String,
    options: String,
    database: String,
    collection: String,
}
impl Connector {
    pub fn new_from_env() -> Result<Self> {
        use crate::envvar;
        let host = envvar::mongodb_host()?;
        let port = envvar::mongodb_port();
        let user = envvar::mongodb_username()?;
        let pass = envvar::mongodb_password()?;
        let urlbase = format!("mongodb://{}:{}@{}:{}", user, pass, host, port);
        let options = envvar::mongodb_options();
        let database = envvar::mongodb_database()?;
        let collection = envvar::mongodb_collection()?;
        //println!("mongodb: urlbase={}, db={}, coll={}", urlbase, database, collection);
        Ok(Connector::new(urlbase, options, database, collection))
    }
    pub fn new(urlbase: String, options: String, database: String, collection: String) -> Self {
        Self {
            urlbase: urlbase,
            options: options,
            database: database,
            collection: collection,
        }
    }
    pub async fn connect(&self) -> Result<Connect> {
        Connect::new(
            &self.urlbase,
            &self.options,
            &self.database,
            &self.collection,
        )
        .await
    }
}

pub struct Connect {
    coll: mongodb::Collection<Document>,
}

impl Connect {
    #[allow(dead_code)]
    pub async fn new_from_env() -> Result<Self> {
        let c = Connector::new_from_env()?;
        c.connect().await
    }
    pub async fn new(
        urlbase: &str,
        options: &str,
        database: &str,
        collection: &str,
    ) -> Result<Self> {
        let url = format!("{}/{}?{}", urlbase, database, options);
        //println!("mongodb: connectiong to '{}'", url);
        let mongodb_client = mongodb::Client::with_uri_str(&url)
            .await
            .known_error(&format!("fail to connect: {}", url.to_string()), true)?;
        let mongodb_coll = mongodb_client.database(database).collection(collection);
        Ok(Self { coll: mongodb_coll })
    }

    #[allow(dead_code)]
    pub async fn delete_all(&mut self) -> Result<()> {
        let _ = self.coll.drop(None).await?;
        Ok(())
    }

    #[cfg(test)]
    pub async fn get_raw_document(&mut self, target_id: &str) -> Result<Option<Document>> {
        let key = db_key(target_id);
        let opt_doc = self
            .coll
            .find_one(key.clone(), None)
            .await
            .known_error("fail to find", true)?;
        Ok(opt_doc)
    }
    pub async fn get_or_default(&mut self, target_id: &str) -> Result<WorkflowRecord> {
        let key = db_key(target_id);
        let workflow_record = WorkflowRecord {
            id: target_id.to_string(),
            works: WorkRecordMap::new(),
        };
        let doc = bson::to_document(&workflow_record)?;
        self.coll
            .update_one(
                key.clone(),
                doc! { "$setOnInsert": doc },
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .known_error("fail to upsert", true)?;

        let opt_doc = self
            .coll
            .find_one(key.clone(), None)
            .await
            .known_error("fail to find", false)?;
        let doc = match opt_doc {
            Some(doc) => Ok(doc),
            None => KnownErrors::normal::<Document>("no document is found", true),
        }?;
        println!("load WorkflowResult: {}", doc);
        let workflow_record = bson::from_document::<WorkflowRecord>(doc)?;
        Ok(workflow_record)
    }
    pub async fn update_work_record(
        &mut self,
        target_id: &str,
        work_record: &WorkRecord,
    ) -> Result<()> {
        let key = db_key(target_id);

        //let mut work_record = work_record_.clone();
        let work_record_doc =
            bson::to_document(&work_record).known_error("fail to serialize WorkRecord", true)?;
        //println!("save WorkRecord: {}", work_record_doc); caution this may be too long

        let _ = self
            .coll
            .update_one(
                key.clone(),
                doc! { "$set": { &format!("works.{}", &work_record.name) : work_record_doc } },
                None,
            )
            .await
            .known_error("fail to update work record", true)?;
        Ok(())
    }
}

pub async fn write_workflow_record<P: AsRef<async_std::path::Path>>(
    workflow_record: &WorkflowRecord,
    path: P,
) -> Result<()> {
    let path = path.as_ref();
    //println!("save to {}...", path.to_str().unwrap());
    let mut file = async_std::fs::File::create(path)
        .await
        .known_error(&format!("fail to create file: {}", path.display()), true)?;
    write(workflow_record, &mut file).await
}

pub async fn write<W: Write + std::marker::Unpin>(
    workflow_record: &WorkflowRecord,
    io: &mut W,
) -> Result<()> {
    //println!("save '{}'...", serde_json::to_string(&doc).unwrap());
    match serde_json::to_string(&workflow_record) {
        Err(e) => Err(e).known_error("fail to serialize to json", false),
        Ok(s) => io
            .write_all(s.as_bytes())
            .await
            .known_error("fail to write", true),
    }?;
    Ok(())
}

pub async fn read_metadata_or_empty<P: AsRef<Path>>(path: P) -> Result<Metadata> {
    let path = path.as_ref();
    let metadata = match path.exists().await {
        false => Metadata::new(),
        true => {
            let mut file = File::open(path)
                .await
                .known_error(&format!("fail to open file: {}", path.display()), true)?;
            read_metadata(&mut file).await?
        }
    };
    Ok(metadata)
}
pub async fn read_metadata<R: Read + std::marker::Unpin>(io: &mut R) -> Result<Metadata> {
    let mut buf = Vec::<u8>::new();
    let _ = io
        .read_to_end(&mut buf)
        .await
        .known_error("fail to read", true)?;

    let s = String::from_utf8(buf).known_error("invalid utf-8 string", true)?;
    let metadata = serde_json::from_str(&s).known_error("malformed json", true)?;
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envvar;
    use crate::error::Result;
    use assert_matches::assert_matches;
    use chrono;
    use serial_test::serial;

    struct Insert {
        pub target_id: String,
        pub work_name: String,
        pub work_version: String,
        pub conn: Connect,
    }
    async fn insert() -> Result<Insert> {
        let target_id = envvar::target_id()?;
        let work_name = envvar::work_name()?;
        let work_version = envvar::work_version()?;

        let mut conn = Connect::new_from_env().await?;
        assert_matches!(conn.delete_all().await, Ok(()));

        let inserted = conn.get_or_default(&target_id).await; //insert
        assert_matches!(inserted, Ok(_));
        Ok(Insert {
            target_id: target_id,
            work_name: work_name,
            work_version: work_version,
            conn: conn,
        })
    }

    #[async_std::test]
    async fn test_insert() -> Result<()> {
        let _ = insert().await?;
        Ok(())
    }

    #[async_std::test]
    #[serial]
    async fn test_insert_and_verify_by_doc() -> Result<()> {
        let mut ins = insert().await?;

        let _wf1 = {
            let doc = ins.conn.get_raw_document(&ins.target_id).await;
            assert_matches!(doc, Ok(Some(_)));
            let doc = doc.unwrap().unwrap();
            let doc_id = doc.get_str("id");
            assert_matches!(doc_id, Ok(_));
            assert_eq!(doc_id.unwrap(), &ins.target_id);
            doc
        };

        let work_record = WorkRecord {
            name: ins.work_name.clone(),
            version: ins.work_version.clone(),
            status: WorkStatus::Succeeded,
            error: None,
            updated: bson::DateTime::from(chrono::Utc::now()),
            metadata: doc! { "hello": "world" },
            artifacts: vec![],
        };
        assert_matches!(
            ins.conn
                .update_work_record(&ins.target_id, &work_record)
                .await,
            Ok(())
        );
        let t1 = bson::DateTime::from(chrono::Utc::now());

        let wf2 = {
            let doc = ins.conn.get_raw_document(&ins.target_id).await;
            assert_matches!(doc, Ok(Some(_)));
            let doc = doc.unwrap().unwrap();
            let doc_id = doc.get_str("id");
            assert_matches!(doc_id, Ok(_));
            assert_eq!(doc_id.unwrap(), &ins.target_id);
            doc
        };
        //println!("{:?}", &wf2);

        let work2 = {
            let works = wf2.get_document("works");
            assert_matches!(works, Ok(_)); //found
            let work = works.unwrap().get_document(&ins.work_name);
            assert_matches!(work, Ok(_)); //found
            work.unwrap()
        };
        assert_matches!(work2.get_str("version"), Ok(_));
        assert_eq!(work2.get_str("version").unwrap(), &ins.work_version);
        let md2 = {
            let md = work2.get_document("metadata");
            assert_matches!(md, Ok(_));
            md.unwrap()
        };
        assert_matches!(md2.get_str("hello"), Ok("world"));
        let work2_updated = {
            let d = work2.get_datetime("updated");
            assert_matches!(d, Ok(_));
            d.unwrap()
        };
        assert!(work2_updated.eq(&work_record.updated));
        assert!(work2_updated.le(&t1));
        Ok(())
    }

    #[async_std::test]
    #[serial]
    async fn test_insert_and_verify_by_struct() -> Result<()> {
        let mut ins = insert().await?;

        let _wf1 = {
            let wf = ins.conn.get_or_default(&ins.target_id).await;
            assert_matches!(wf, Ok(_));
            let wf = wf.unwrap();
            assert_eq!(&wf.id, &ins.target_id);
            wf
        };

        let work_record = WorkRecord {
            name: ins.work_name.clone(),
            version: ins.work_version.clone(),
            status: WorkStatus::Succeeded,
            error: None,
            updated: bson::DateTime::from(chrono::Utc::now()),
            metadata: doc! { "hello": "world" },
            artifacts: vec![],
        };
        assert_matches!(
            ins.conn
                .update_work_record(&ins.target_id, &work_record)
                .await,
            Ok(())
        );
        let t1 = bson::DateTime::from(chrono::Utc::now());

        let wf2 = {
            let wf = ins.conn.get_or_default(&ins.target_id).await;
            assert_matches!(wf, Ok(_));
            let wf = wf.unwrap();
            assert_eq!(&wf.id, &ins.target_id);
            wf
        };
        //println!("{:?}", &wf2);
        let work2 = {
            let work = wf2.works.get(&ins.work_name);
            assert_matches!(work, Some(_)); //found
            work.unwrap()
        };
        assert_eq!(&work2.version, &ins.work_version);
        let md2 = &work2.metadata;
        assert_matches!(md2.get("hello"), Some(_));
        assert_eq!(md2.get_str("hello").unwrap(), "world");
        let work2_updated = work2.updated;
        assert!(work2_updated.eq(&work_record.updated));
        assert!(work2_updated.le(&t1));
        Ok(())
    }
}
