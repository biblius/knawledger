use async_recursion::async_recursion;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{debug, error, info};

use crate::db::Database;
use crate::error::KnawledgeError;
use crate::{FILES_PER_THREAD, MAX_THREADS};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::thread::ScopedJoinHandle;
use std::time::Instant;
use std::{fmt::Debug, path::Path};

/// Database model
#[derive(Debug, Default)]
pub struct Document {
    /// File name with extension
    pub file_name: String,
    /// Directory ID that contains this file
    pub directory: uuid::Uuid,
    /// Canonicalised path
    pub path: String,
}

impl Document {
    pub fn new(
        directory: uuid::Uuid,
        name: String,
        path: String,
    ) -> Result<(Self, DocumentMeta), KnawledgeError> {
        debug!("Processing {path}");

        let meta = DocumentMeta::read_from_file(&path)?;

        let document = Self {
            file_name: name,
            directory,
            path,
        };

        Ok((document, meta))
    }

    pub fn name(path: impl AsRef<Path>) -> String {
        path.as_ref()
            .file_name()
            .unwrap_or(OsStr::new("__unknown"))
            .to_str()
            .unwrap_or("__unknown")
            .to_string()
    }
}

/// Document read from the fs with its metadata.
#[derive(Debug, Default)]
pub struct DocumentData {
    /// Document markdown content
    pub content: String,
    pub meta: DocumentMeta,
}

impl DocumentData {
    pub fn read_from_disk(path: impl AsRef<Path>) -> Result<Self, KnawledgeError> {
        let mut data = Self::default();
        let content = fs::read_to_string(path)?;
        let (meta, content) = DocumentMeta::read_from_str(&content)?;
        data.content = content.to_string();
        data.meta = meta;
        Ok(data)
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct DocumentMeta {
    /// A user specified identifier for the document for
    /// URLs on Knawledger. Prioritised over the document UUID.
    #[serde(rename = "id")]
    pub custom_id: Option<String>,
    pub title: Option<String>,
    pub reading_time: Option<i32>,
    pub tags: Option<Vec<String>>,
}

impl DocumentMeta {
    pub fn read_from_file(path: impl AsRef<Path>) -> Result<Self, KnawledgeError> {
        let content = fs::read_to_string(path)?;
        Ok(Self::read_from_str(&content)?.0)
    }

    /// Used when we already read the file from the fs.
    /// Returns the read meta and the remainder of the content.
    pub fn read_from_str(content: &str) -> Result<(Self, &str), KnawledgeError> {
        let mut data = Self {
            title: find_title_from_h1(content),
            ..Default::default()
        };

        if !content.starts_with("---") {
            return Ok((data, content));
        }

        if content.len() < 4 {
            return Ok((data, content));
        }

        let Some(end_i) = &content[3..].find("---") else {
            return Ok((data, &content[3..]));
        };

        // Offset to account for the skipped ---
        let meta_str = &content[3..*end_i + 2];

        if meta_str.is_empty() {
            return Ok((data, &content[end_i + 6..]));
        }

        data = serde_yaml::from_str(meta_str)?;

        let content = &content[end_i + 6..];

        data.reading_time = Some(calculate_reading_time(content));

        if data.title.is_none() {
            data.title = find_title_from_h1(content);
        }

        Ok((data, content))
    }
}

#[derive(Debug, Default)]
pub struct Directory {
    pub id: uuid::Uuid,
    pub name: String,
    pub parent: Option<uuid::Uuid>,
    pub path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[async_recursion]
pub async fn process_directory(
    db: &Database,
    path: impl AsRef<Path> + 'async_recursion + Send,
    parent: Option<uuid::Uuid>,
) -> Result<(), KnawledgeError> {
    let entries = fs::read_dir(&path)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    let full_path = path.as_ref().canonicalize()?.display().to_string();
    let dir_name = path
        .as_ref()
        .file_name()
        .ok_or(KnawledgeError::InvalidDirectory(format!(
            "{full_path}: unsupported directory"
        )))?;

    debug!("Loading {full_path}");

    let dir_name = dir_name
        .to_str()
        .ok_or(KnawledgeError::InvalidDirectory(format!(
            "{dir_name:?}: not valid utf-8"
        )))?;

    let directory_entry: Directory = match parent {
        Some(parent_id) => {
            let parent = db.get_dir_by_name_and_parent(dir_name, parent_id).await?;

            match parent {
                Some(dir) => dir,
                None => db.insert_dir(&full_path, dir_name, Some(parent_id)).await?,
            }
        }
        None => {
            let root = db.get_root_dir_by_name(dir_name).await?;
            match root {
                Some(dir) => dir,
                None => db.insert_dir(&full_path, dir_name, None).await?,
            }
        }
    };

    for entry in entries.iter() {
        if entry.path().is_dir() {
            process_directory(db, entry.path(), Some(directory_entry.id)).await?;
        }
    }

    let mut files_processed = vec![];
    let mut markdown_files = vec![];
    let mut file_names = vec![];

    for entry in entries.iter() {
        let path = entry.path();
        let Some(ext) = path.extension() else {
            continue;
        };

        let Some(ext) = ext.to_str() else {
            continue;
        };

        if ext != "md" {
            continue;
        }

        if let Some(name) = path.file_name() {
            if let Some(name) = name.to_str() {
                file_names.push(name.to_string());
            }
        }
        markdown_files.push(path);
    }

    let existing = db
        .list_document_in_dir(directory_entry.id, &file_names)
        .await?;

    let mut amt_files_existing = 0;
    for item in existing {
        let idx = markdown_files.iter().position(|el| {
            let Some(file_name) = el.iter().last() else {
                return false;
            };

            let Some(file_name) = file_name.to_str() else {
                return false;
            };

            item.file_name == file_name
        });

        if let Some(idx) = idx {
            debug!("Already exists: {}", item.file_name);
            markdown_files.swap_remove(idx);
            amt_files_existing += 1;
        }
    }

    process_files(directory_entry.id, markdown_files, &mut files_processed)?;

    let amt_files_processed = files_processed.len();
    for (file, meta) in files_processed {
        db.insert_doc(&file, &meta).await?;
    }

    info!(
        "{full_path} - Existing files: {amt_files_existing} Processed files: {amt_files_processed}",
    );

    Ok(())
}

fn process_files(
    directory: uuid::Uuid,
    file_paths: Vec<PathBuf>,
    files: &mut Vec<(Document, DocumentMeta)>,
) -> Result<(), KnawledgeError> {
    let files_total = file_paths.len();

    let mut files_remaining = files_total;

    while files_remaining > 0 {
        let mut batches: Vec<&[PathBuf]> = vec![&[]; *MAX_THREADS];

        for (i, batch) in batches.iter_mut().enumerate() {
            let start = i * FILES_PER_THREAD;

            let mut end = (i + 1) * FILES_PER_THREAD;

            if end > files_total {
                end = files_total;

                *batch = &file_paths[start..end];

                files_remaining -= end - start;

                break;
            }

            *batch = &file_paths[start..end];

            files_remaining -= FILES_PER_THREAD;
        }

        type TaskWithStart<'a> = (
            ScopedJoinHandle<'a, Result<Vec<(Document, DocumentMeta)>, KnawledgeError>>,
            Instant,
        );

        batches.retain(|batch| !batch.is_empty());

        if batches.len() > 1 {
            debug!("Processing multiple batches");
            std::thread::scope(|scope| {
                let mut tasks: Vec<TaskWithStart> = Vec::with_capacity(*MAX_THREADS);

                for batch in batches {
                    if batch.is_empty() {
                        continue;
                    }

                    let task = scope.spawn(move || {
                        let mut files = vec![];
                        for file_path in batch {
                            let file = Document::new(
                                directory,
                                Document::name(file_path.canonicalize()?),
                                file_path.display().to_string(),
                            )?;
                            files.push(file);
                        }
                        Ok(files)
                    });

                    debug!("Spawned thread {:?}", task.thread().id());

                    tasks.push((task, Instant::now()));
                }

                for (task, start) in tasks {
                    let id = task.thread().id();
                    let result = task.join();
                    match result {
                        Ok(Ok(processed)) => {
                            files.extend(processed);
                            debug!(
                                "Thread {:?} finished in {}ms",
                                id,
                                Instant::now().duration_since(start).as_nanos() as f32 * 0.001
                            );
                        }
                        Ok(Err(e)) => error!("Error occurred while processing files: {e:?}"),
                        Err(e) => error!("Error occurred while processing files: {e:?}"),
                    }
                }
            });
        } else {
            debug!("Processing single batch");
            for file_path in batches[0] {
                let file = Document::new(
                    directory,
                    Document::name(file_path),
                    file_path.canonicalize()?.display().to_string(),
                )?;
                files.push(file);
            }
        }
    }

    Ok(())
}

pub fn find_title_from_h1(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        let Some((_, title)) = line.split_once('#') else {
            continue;
        };

        return Some(title.trim().to_string());
    }

    None
}

fn calculate_reading_time(content: &str) -> i32 {
    let words = content.split(' ').collect::<Vec<_>>().len();
    ((words / 200) as f32 * 0.60) as i32
}
