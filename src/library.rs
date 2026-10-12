use std::{
    error::Error,
    fmt::Write as _,
    fs::File,
    io::{BufReader, Read},
    path::Path,
    time::UNIX_EPOCH,
};

use rusqlite::{Connection, OptionalExtension, Result, params};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub type ImportResult<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImportSummary {
    pub imported: usize,
    pub unchanged: usize,
    pub skipped: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemKind {
    Folder,
    Book,
    Comic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LibraryItem {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub format: String,
    pub kind: ItemKind,
    pub favorite: bool,
    pub progress: Option<f64>,
    pub thumbnail_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThumbnailCandidate {
    pub id: String,
    pub source: String,
    pub format: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TitleSource {
    pub id: String,
    pub title: String,
    pub source: String,
    pub format: String,
    pub progress_locator: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub locator: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReaderPreferences {
    pub font_size: f32,
    pub theme: i32,
    pub margin: f32,
    pub line_spacing: f32,
}

impl Default for ReaderPreferences {
    fn default() -> Self {
        Self {
            font_size: 18.0,
            theme: 0,
            margin: 18.0,
            line_spacing: 1.2,
        }
    }
}

pub struct LibraryDb {
    connection: Connection,
}

impl LibraryDb {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;

        let mut database = Self { connection };
        database.apply_migrations()?;
        Ok(database)
    }

    fn apply_migrations(&mut self) -> Result<()> {
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;

        if version < 1 {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(
                "CREATE TABLE folders (
                    id          INTEGER PRIMARY KEY,
                    name        TEXT NOT NULL,
                    parent_id   INTEGER REFERENCES folders(id) ON DELETE SET NULL,
                    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
                );

                CREATE INDEX folders_parent_idx ON folders(parent_id);

                CREATE TABLE titles (
                    id              TEXT PRIMARY KEY,
                    source          TEXT NOT NULL UNIQUE,
                    title           TEXT NOT NULL,
                    author          TEXT NOT NULL DEFAULT '',
                    format          TEXT NOT NULL CHECK (format IN ('EPUB', 'CBR', 'CBZ', 'TXT')),
                    file_size       INTEGER NOT NULL DEFAULT 0,
                    modified_at     INTEGER,
                    imported_at     INTEGER NOT NULL DEFAULT (unixepoch())
                );

                CREATE TABLE folder_titles (
                    folder_id   INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                    title_id    TEXT NOT NULL REFERENCES titles(id) ON DELETE CASCADE,
                    PRIMARY KEY (folder_id, title_id)
                );

                CREATE INDEX folder_titles_title_idx ON folder_titles(title_id);

                CREATE TABLE reading_state (
                    title_id             TEXT PRIMARY KEY REFERENCES titles(id) ON DELETE CASCADE,
                    favorite             INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
                    progress_locator     TEXT,
                    progress_fraction    REAL CHECK (
                        progress_fraction IS NULL OR
                        (progress_fraction >= 0.0 AND progress_fraction <= 1.0)
                    ),
                    is_reading           INTEGER NOT NULL DEFAULT 0 CHECK (is_reading IN (0, 1)),
                    progress_updated_at  INTEGER
                );

                CREATE TABLE bookmarks (
                    id          INTEGER PRIMARY KEY,
                    title_id    TEXT NOT NULL REFERENCES titles(id) ON DELETE CASCADE,
                    locator     TEXT NOT NULL,
                    label       TEXT NOT NULL DEFAULT '',
                    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
                );

                PRAGMA user_version = 1;",
            )?;
            transaction.commit()?;
        }

        if version < 2 {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(
                "CREATE UNIQUE INDEX IF NOT EXISTS folders_unique_location_idx
                 ON folders(COALESCE(parent_id, -1), name COLLATE NOCASE);

                 PRAGMA user_version = 2;",
            )?;
            transaction.commit()?;
        }

        if version < 3 {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(
                "ALTER TABLE titles
                 ADD COLUMN in_library_root INTEGER NOT NULL DEFAULT 0
                 CHECK (in_library_root IN (0, 1));

                 UPDATE titles
                 SET in_library_root = 1
                 WHERE NOT EXISTS (
                    SELECT 1 FROM folder_titles ft WHERE ft.title_id = titles.id
                 );

                 CREATE INDEX titles_library_root_idx ON titles(in_library_root);

                 PRAGMA user_version = 3;",
            )?;
            transaction.commit()?;
        }

        if version < 4 {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(
                "ALTER TABLE reading_state
                 ADD COLUMN reader_layout TEXT NOT NULL DEFAULT 'auto'
                 CHECK (reader_layout IN ('auto', 'original', 'reflowable'));

                 PRAGMA user_version = 4;",
            )?;
            transaction.commit()?;
        }

        if version < 5 {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(
                "DELETE FROM bookmarks
                 WHERE id NOT IN (
                    SELECT MIN(id) FROM bookmarks GROUP BY title_id, locator
                 );

                 CREATE UNIQUE INDEX bookmarks_title_locator_idx
                 ON bookmarks(title_id, locator);

                 PRAGMA user_version = 5;",
            )?;
            transaction.commit()?;
        }

        if version < 6 {
            let defaults = ReaderPreferences::default();
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(
                "CREATE TABLE reader_preferences (
                    id              INTEGER PRIMARY KEY CHECK (id = 1),
                    font_size       REAL NOT NULL,
                    theme           INTEGER NOT NULL CHECK (theme BETWEEN 0 AND 2),
                    margin          REAL NOT NULL,
                    line_spacing    REAL NOT NULL
                 );",
            )?;
            transaction.execute(
                "INSERT INTO reader_preferences
                 (id, font_size, theme, margin, line_spacing)
                 VALUES (1, ?1, ?2, ?3, ?4)",
                params![
                    defaults.font_size,
                    defaults.theme,
                    defaults.margin,
                    defaults.line_spacing
                ],
            )?;
            transaction.pragma_update(None, "user_version", 6)?;
            transaction.commit()?;
        }

        if version < 7 {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(
                "ALTER TABLE titles ADD COLUMN thumbnail_path TEXT;
                 PRAGMA user_version = 7;",
            )?;
            transaction.commit()?;
        }

        self.repair_legacy_android_titles()?;

        Ok(())
    }

    fn repair_legacy_android_titles(&self) -> Result<()> {
        let mut statement = self.connection.prepare(
            "SELECT id, title FROM titles
             WHERE source LIKE '%/saf-inbox/%'",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let repairs = rows
            .filter_map(std::result::Result::ok)
            .filter_map(|(id, title)| cleaned_android_title(&title).map(|clean| (id, clean)))
            .collect::<Vec<_>>();
        drop(statement);
        for (id, title) in repairs {
            self.connection.execute(
                "UPDATE titles SET title = ?1 WHERE id = ?2",
                params![title, id],
            )?;
        }
        Ok(())
    }

    pub fn seed_demo_if_empty(&mut self) -> Result<()> {
        let title_count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM titles", [], |row| row.get(0))?;
        if title_count != 0 {
            return Ok(());
        }

        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO folders (id, name, parent_id) VALUES (1, 'Fantasy', NULL)",
            [],
        )?;
        transaction.execute(
            "INSERT INTO folders (id, name, parent_id) VALUES (2, 'Graphic novels', NULL)",
            [],
        )?;
        transaction.execute(
            "INSERT INTO folders (id, name, parent_id) VALUES (3, 'Epic sagas', 1)",
            [],
        )?;

        let demo_titles = [
            (
                "demo-grad-magla",
                "demo://grad-od-magle.epub",
                "City of Mist",
                "Mila Petrović",
                "EPUB",
            ),
            (
                "demo-posljednja-stanica",
                "demo://posljednja-stanica.txt",
                "Last Stop",
                "Nikola Ilić",
                "TXT",
            ),
            (
                "demo-svemirska-patrola-01",
                "demo://svemirska-patrola-01.cbz",
                "Space Patrol 01",
                "Sample comic",
                "CBZ",
            ),
            (
                "demo-hronike-sjevera",
                "demo://hronike-sjevera.epub",
                "Chronicles of the North",
                "Jelena Marković",
                "EPUB",
            ),
            (
                "demo-zmaj-od-papira",
                "demo://zmaj-od-papira.epub",
                "Paper Dragon",
                "Vuk Stanić",
                "EPUB",
            ),
            (
                "demo-cuvari-prolaza",
                "demo://cuvari-prolaza.cbr",
                "Guardians of the Passage",
                "Sample comic",
                "CBR",
            ),
        ];

        for (id, source, title, author, format) in demo_titles {
            transaction.execute(
                "INSERT INTO titles (id, source, title, author, format)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, source, title, author, format],
            )?;
            transaction.execute("INSERT INTO reading_state (title_id) VALUES (?1)", [id])?;
        }

        for (folder_id, title_id) in [
            (1, "demo-hronike-sjevera"),
            (1, "demo-zmaj-od-papira"),
            (1, "demo-cuvari-prolaza"),
            (2, "demo-svemirska-patrola-01"),
            (2, "demo-cuvari-prolaza"),
            (3, "demo-hronike-sjevera"),
        ] {
            transaction.execute(
                "INSERT INTO folder_titles (folder_id, title_id) VALUES (?1, ?2)",
                params![folder_id, title_id],
            )?;
        }

        transaction.execute(
            "UPDATE reading_state SET favorite = 1 WHERE title_id IN
             ('demo-grad-magla', 'demo-svemirska-patrola-01')",
            [],
        )?;
        transaction.execute(
            "UPDATE titles SET in_library_root = 1 WHERE id IN
             ('demo-grad-magla', 'demo-posljednja-stanica', 'demo-svemirska-patrola-01')",
            [],
        )?;
        transaction.execute(
            "UPDATE reading_state
             SET progress_locator = 'char:1840', progress_fraction = 0.34,
                 is_reading = 1, progress_updated_at = unixepoch()
             WHERE title_id = 'demo-posljednja-stanica'",
            [],
        )?;
        transaction.execute(
            "UPDATE reading_state
             SET progress_locator = 'chapter:2:block:7', progress_fraction = 0.12,
                 is_reading = 1, progress_updated_at = unixepoch()
             WHERE title_id = 'demo-hronike-sjevera'",
            [],
        )?;

        transaction.commit()?;
        Ok(())
    }

    pub fn root_items(&self) -> Result<Vec<LibraryItem>> {
        let mut items = self.folders_with_parent(None)?;
        items.extend(self.query_titles("WHERE t.in_library_root = 1", [])?);
        Ok(items)
    }

    pub fn folder_items(&self, folder_id: i64) -> Result<Vec<LibraryItem>> {
        let mut items = self.folders_with_parent(Some(folder_id))?;
        items.extend(self.query_titles(
            "JOIN folder_titles ft ON ft.title_id = t.id WHERE ft.folder_id = ?1",
            [folder_id],
        )?);
        Ok(items)
    }

    fn folders_with_parent(&self, parent_id: Option<i64>) -> Result<Vec<LibraryItem>> {
        let mut statement = self.connection.prepare(
            "SELECT f.id, f.name,
                    (SELECT COUNT(*) FROM folders c WHERE c.parent_id = f.id) +
                    (SELECT COUNT(*) FROM folder_titles ft WHERE ft.folder_id = f.id)
             FROM folders f
             WHERE (f.parent_id = ?1 OR (f.parent_id IS NULL AND ?1 IS NULL))
             ORDER BY f.name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([parent_id], |row| {
            let count: i64 = row.get(2)?;
            Ok(LibraryItem {
                id: format!("folder:{}", row.get::<_, i64>(0)?),
                title: row.get(1)?,
                subtitle: if count == 1 {
                    "1 item".to_owned()
                } else {
                    format!("{count} items")
                },
                format: "FOLDER".to_owned(),
                kind: ItemKind::Folder,
                favorite: false,
                progress: None,
                thumbnail_path: None,
            })
        })?;
        rows.collect()
    }

    pub fn favorite_items(&self) -> Result<Vec<LibraryItem>> {
        self.query_titles("WHERE COALESCE(rs.favorite, 0) = 1", [])
    }

    pub fn reading_items(&self) -> Result<Vec<LibraryItem>> {
        self.query_titles(
            "WHERE COALESCE(rs.is_reading, 0) = 1 ORDER BY rs.progress_updated_at DESC",
            [],
        )
    }

    pub fn search_items(&self, query: &str) -> Result<Vec<LibraryItem>> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        Ok(self
            .query_titles("ORDER BY t.title COLLATE NOCASE", [])?
            .into_iter()
            .filter(|item| {
                item.title.to_lowercase().contains(&query)
                    || item.subtitle.to_lowercase().contains(&query)
            })
            .collect())
    }

    fn query_titles<P>(&self, clause: &str, parameters: P) -> Result<Vec<LibraryItem>>
    where
        P: rusqlite::Params,
    {
        let sql = format!(
            "SELECT t.id, t.title, t.author, t.format,
                    COALESCE(rs.favorite, 0), rs.progress_fraction, t.thumbnail_path
             FROM titles t
             LEFT JOIN reading_state rs ON rs.title_id = t.id
             {clause}"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(parameters, |row| {
            let format: String = row.get(3)?;
            Ok(LibraryItem {
                id: row.get(0)?,
                title: row.get(1)?,
                subtitle: {
                    let author: String = row.get(2)?;
                    if author.trim().is_empty() {
                        "Unknown author".to_owned()
                    } else {
                        author
                    }
                },
                kind: if matches!(format.as_str(), "CBR" | "CBZ") {
                    ItemKind::Comic
                } else {
                    ItemKind::Book
                },
                format,
                favorite: row.get(4)?,
                progress: row.get(5)?,
                thumbnail_path: row.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn folder_name(&self, folder_id: i64) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT name FROM folders WHERE id = ?1",
                [folder_id],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn title_source(&self, title_id: &str) -> Result<Option<TitleSource>> {
        self.connection
            .query_row(
                "SELECT t.id, t.title, t.source, t.format, rs.progress_locator
                 FROM titles t
                 LEFT JOIN reading_state rs ON rs.title_id = t.id
                 WHERE t.id = ?1",
                [title_id],
                |row| {
                    Ok(TitleSource {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        source: row.get(2)?,
                        format: row.get(3)?,
                        progress_locator: row.get(4)?,
                    })
                },
            )
            .optional()
    }

    pub fn create_folder(&self, name: &str, parent_id: Option<i64>) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "Folder name cannot be empty".to_owned(),
            ));
        }

        if let Some(id) = self
            .connection
            .query_row(
                "SELECT id FROM folders
                 WHERE name = ?1 COLLATE NOCASE
                   AND (parent_id = ?2 OR (parent_id IS NULL AND ?2 IS NULL))",
                params![name, parent_id],
                |row| row.get(0),
            )
            .optional()?
        {
            return Ok(id);
        }

        self.connection.execute(
            "INSERT INTO folders (name, parent_id) VALUES (?1, ?2)",
            params![name, parent_id],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn rename_folder(&self, folder_id: i64, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "Folder name cannot be empty".to_owned(),
            ));
        }
        self.connection.execute(
            "UPDATE folders SET name = ?1 WHERE id = ?2",
            params![name, folder_id],
        )?;
        Ok(())
    }

    pub fn import_file(&mut self, path: &Path, folder_id: Option<i64>) -> ImportResult<bool> {
        self.import_file_named(path, folder_id, None)
    }

    pub fn import_file_named(
        &mut self,
        path: &Path,
        folder_id: Option<i64>,
        display_name: Option<&str>,
    ) -> ImportResult<bool> {
        let path = path.canonicalize()?;
        let Some(format) = supported_format(&path) else {
            return Ok(false);
        };
        let metadata = path.metadata()?;
        if !metadata.is_file() {
            return Ok(false);
        }
        let file_size = i64::try_from(metadata.len())?;
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_secs()).ok());
        let source = path.to_string_lossy().into_owned();

        let unchanged_id: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM titles
                 WHERE source = ?1 AND file_size = ?2
                   AND (modified_at = ?3 OR (modified_at IS NULL AND ?3 IS NULL))",
                params![source, file_size, modified_at],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(title_id) = unchanged_id {
            if let Some(folder_id) = folder_id {
                self.connection.execute(
                    "INSERT OR IGNORE INTO folder_titles (folder_id, title_id) VALUES (?1, ?2)",
                    params![folder_id, title_id],
                )?;
            } else {
                self.connection.execute(
                    "UPDATE titles SET in_library_root = 1 WHERE id = ?1",
                    [title_id],
                )?;
            }
            return Ok(false);
        }

        let content_id = hash_file(&path)?;
        let title = display_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .and_then(|name| Path::new(name).file_stem().and_then(|stem| stem.to_str()))
            .or_else(|| path.file_stem().and_then(|name| name.to_str()))
            .unwrap_or("Untitled")
            .to_owned();
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "DELETE FROM titles WHERE source = ?1 AND id <> ?2",
            params![source, content_id],
        )?;
        transaction.execute(
            "INSERT INTO titles (
                id, source, title, author, format, file_size, modified_at, in_library_root
             )
             VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                source = excluded.source,
                title = excluded.title,
                format = excluded.format,
                file_size = excluded.file_size,
                modified_at = excluded.modified_at,
                in_library_root = MAX(in_library_root, excluded.in_library_root)",
            params![
                content_id,
                source,
                title,
                format,
                file_size,
                modified_at,
                i64::from(folder_id.is_none())
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO reading_state (title_id) VALUES (?1)",
            [&content_id],
        )?;
        if let Some(folder_id) = folder_id {
            transaction.execute(
                "INSERT OR IGNORE INTO folder_titles (folder_id, title_id) VALUES (?1, ?2)",
                params![folder_id, content_id],
            )?;
        }
        transaction.commit()?;
        Ok(true)
    }

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub fn import_reference(
        &mut self,
        source: &str,
        display_name: &str,
        folder_id: Option<i64>,
    ) -> ImportResult<bool> {
        let Some(format) = supported_format(Path::new(display_name)) else {
            return Ok(false);
        };
        let title = Path::new(display_name)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .trim();
        let mut hasher = Sha256::new();
        hasher.update(source.as_bytes());
        let mut content_id = String::from("uri:");
        for byte in hasher.finalize() {
            write!(&mut content_id, "{byte:02x}")?;
        }
        let transaction = self.connection.transaction()?;
        let existed: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM titles WHERE id = ?1)",
            [&content_id],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO titles (
                id, source, title, author, format, file_size, modified_at, in_library_root
             ) VALUES (?1, ?2, ?3, '', ?4, 0, NULL, ?5)
             ON CONFLICT(id) DO UPDATE SET
                source = excluded.source,
                title = excluded.title,
                format = excluded.format,
                in_library_root = MAX(in_library_root, excluded.in_library_root)",
            params![
                content_id,
                source,
                title,
                format,
                i64::from(folder_id.is_none())
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO reading_state (title_id) VALUES (?1)",
            [&content_id],
        )?;
        if let Some(folder_id) = folder_id {
            transaction.execute(
                "INSERT OR IGNORE INTO folder_titles (folder_id, title_id) VALUES (?1, ?2)",
                params![folder_id, content_id],
            )?;
        }
        transaction.commit()?;
        Ok(!existed)
    }

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub fn create_unique_root_folder(&self, display_name: &str) -> Result<i64> {
        let root_name = self.available_folder_name(display_name.trim(), None)?;
        self.create_folder(&root_name, None)
    }

    pub fn thumbnail_candidates(&self) -> Result<Vec<ThumbnailCandidate>> {
        let mut statement = self.connection.prepare(
            "SELECT id, source, format FROM titles
             WHERE thumbnail_path IS NULL
               AND id NOT LIKE 'demo-%'
             ORDER BY imported_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ThumbnailCandidate {
                id: row.get(0)?,
                source: row.get(1)?,
                format: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn set_thumbnail_path(&self, title_id: &str, path: Option<&Path>) -> Result<()> {
        let path = path.map(|value| value.to_string_lossy().into_owned());
        self.connection.execute(
            "UPDATE titles SET thumbnail_path = ?1 WHERE id = ?2",
            params![path, title_id],
        )?;
        Ok(())
    }

    #[cfg_attr(target_os = "android", allow(dead_code))]
    pub fn import_directory(
        &mut self,
        directory: &Path,
        parent_id: Option<i64>,
    ) -> ImportResult<ImportSummary> {
        let directory = directory.canonicalize()?;
        let root_name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Imported folder");
        let root_folder = self.create_folder(root_name, parent_id)?;
        self.import_directory_contents(&directory, root_folder)
    }

    /// Imports a newly selected directory without ever merging it into an
    /// earlier root folder that happens to have the same display name.
    #[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
    pub fn import_directory_as_new(
        &mut self,
        directory: &Path,
        parent_id: Option<i64>,
        display_name: &str,
    ) -> ImportResult<ImportSummary> {
        let directory = directory.canonicalize()?;
        let base_name = if display_name.trim().is_empty() {
            directory
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Imported folder")
        } else {
            display_name.trim()
        };
        let root_name = self.available_folder_name(base_name, parent_id)?;
        let root_folder = self.create_folder(&root_name, parent_id)?;
        self.import_directory_contents(&directory, root_folder)
    }

    #[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
    fn available_folder_name(&self, base_name: &str, parent_id: Option<i64>) -> Result<String> {
        let mut candidate = base_name.to_owned();
        let mut suffix = 2;
        while self
            .connection
            .query_row(
                "SELECT 1 FROM folders
                 WHERE name = ?1 COLLATE NOCASE
                   AND (parent_id = ?2 OR (parent_id IS NULL AND ?2 IS NULL))",
                params![candidate, parent_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            candidate = format!("{base_name} ({suffix})");
            suffix += 1;
        }
        Ok(candidate)
    }

    fn import_directory_contents(
        &mut self,
        directory: &Path,
        root_folder: i64,
    ) -> ImportResult<ImportSummary> {
        let mut folders = std::collections::HashMap::from([(directory.to_path_buf(), root_folder)]);
        let mut summary = ImportSummary::default();

        for entry in WalkDir::new(&directory).min_depth(1).sort_by_file_name() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    summary.skipped += 1;
                    continue;
                }
            };
            let path = entry.path();
            if entry.file_type().is_dir() {
                let Some(parent_path) = path.parent() else {
                    summary.skipped += 1;
                    continue;
                };
                let Some(parent_folder) = folders.get(parent_path).copied() else {
                    summary.skipped += 1;
                    continue;
                };
                let name = entry.file_name().to_string_lossy();
                let folder_id = self.create_folder(&name, Some(parent_folder))?;
                folders.insert(path.to_path_buf(), folder_id);
            } else if entry.file_type().is_file() {
                if supported_format(path).is_none() {
                    summary.skipped += 1;
                    continue;
                }
                let folder_id = path
                    .parent()
                    .and_then(|parent| folders.get(parent))
                    .copied();
                if self.import_file(path, folder_id)? {
                    summary.imported += 1;
                } else {
                    summary.unchanged += 1;
                }
            }
        }

        Ok(summary)
    }

    pub fn toggle_favorite(&self, title_id: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO reading_state (title_id, favorite)
             VALUES (?1, 1)
             ON CONFLICT(title_id) DO UPDATE SET favorite = 1 - favorite",
            [title_id],
        )?;
        Ok(())
    }

    pub fn remove_from_reading(&self, title_id: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE reading_state SET is_reading = 0 WHERE title_id = ?1",
            [title_id],
        )?;
        Ok(())
    }

    pub fn record_progress(&self, title_id: &str, locator: &str, fraction: f64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO reading_state (
                title_id, progress_locator, progress_fraction, is_reading, progress_updated_at
             ) VALUES (?1, ?2, ?3, 1, unixepoch())
             ON CONFLICT(title_id) DO UPDATE SET
                progress_locator = excluded.progress_locator,
                progress_fraction = excluded.progress_fraction,
                is_reading = 1,
                progress_updated_at = unixepoch()",
            params![title_id, locator, fraction],
        )?;
        Ok(())
    }

    pub fn toggle_bookmark(&self, title_id: &str, locator: &str, label: &str) -> Result<bool> {
        let removed = self.connection.execute(
            "DELETE FROM bookmarks WHERE title_id = ?1 AND locator = ?2",
            params![title_id, locator],
        )?;
        if removed > 0 {
            return Ok(false);
        }
        self.connection.execute(
            "INSERT INTO bookmarks (title_id, locator, label) VALUES (?1, ?2, ?3)",
            params![title_id, locator, label],
        )?;
        Ok(true)
    }

    pub fn bookmarks(&self, title_id: &str) -> Result<Vec<Bookmark>> {
        let mut statement = self.connection.prepare(
            "SELECT locator, label FROM bookmarks
             WHERE title_id = ?1 ORDER BY created_at, id",
        )?;
        statement
            .query_map([title_id], |row| {
                Ok(Bookmark {
                    locator: row.get(0)?,
                    label: row.get(1)?,
                })
            })?
            .collect()
    }

    pub fn is_bookmarked(&self, title_id: &str, locator: &str) -> Result<bool> {
        self.connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM bookmarks WHERE title_id = ?1 AND locator = ?2
             )",
            params![title_id, locator],
            |row| row.get(0),
        )
    }

    pub fn reader_preferences(&self) -> Result<ReaderPreferences> {
        self.connection.query_row(
            "SELECT font_size, theme, margin, line_spacing
             FROM reader_preferences WHERE id = 1",
            [],
            |row| {
                Ok(ReaderPreferences {
                    font_size: row.get(0)?,
                    theme: row.get(1)?,
                    margin: row.get(2)?,
                    line_spacing: row.get(3)?,
                })
            },
        )
    }

    pub fn set_reader_preferences(&self, preferences: ReaderPreferences) -> Result<()> {
        if !(12.0..=32.0).contains(&preferences.font_size)
            || !(0..=2).contains(&preferences.theme)
            || !(0.0..=72.0).contains(&preferences.margin)
            || !(1.0..=2.0).contains(&preferences.line_spacing)
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "Invalid reader settings".to_owned(),
            ));
        }
        self.connection.execute(
            "UPDATE reader_preferences
             SET font_size = ?1, theme = ?2, margin = ?3, line_spacing = ?4
             WHERE id = 1",
            params![
                preferences.font_size,
                preferences.theme,
                preferences.margin,
                preferences.line_spacing
            ],
        )?;
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn delete_folder(&self, folder_id: i64) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute("DELETE FROM folders WHERE id = ?1", [folder_id])?;
        transaction.execute(
            "DELETE FROM titles
             WHERE in_library_root = 0
               AND NOT EXISTS (
                    SELECT 1 FROM folder_titles ft WHERE ft.title_id = titles.id
               )",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_title_from_folder(&self, folder_id: i64, title_id: &str) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM folder_titles WHERE folder_id = ?1 AND title_id = ?2",
            params![folder_id, title_id],
        )?;
        transaction.execute(
            "DELETE FROM titles
             WHERE id = ?1 AND in_library_root = 0
               AND NOT EXISTS (
                    SELECT 1 FROM folder_titles ft WHERE ft.title_id = titles.id
               )",
            [title_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_title_from_library(&self, title_id: &str) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE titles SET in_library_root = 0 WHERE id = ?1",
            [title_id],
        )?;
        transaction.execute(
            "DELETE FROM titles
             WHERE id = ?1
               AND NOT EXISTS (
                    SELECT 1 FROM folder_titles ft WHERE ft.title_id = titles.id
               )",
            [title_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    #[cfg(test)]
    fn title_exists(&self, title_id: &str) -> Result<bool> {
        self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM titles WHERE id = ?1)",
            [title_id],
            |row| row.get(0),
        )
    }
}

fn supported_format(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "epub" => Some("EPUB"),
        "cbr" => Some("CBR"),
        "cbz" => Some("CBZ"),
        "txt" => Some("TXT"),
        _ => None,
    }
}

fn cleaned_android_title(title: &str) -> Option<String> {
    let digit_count = title.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count < 10 {
        return None;
    }
    let cleaned = title[digit_count..]
        .trim_start_matches(['-', '_', ' '])
        .trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

fn hash_file(path: &Path) -> ImportResult<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let mut content_id = String::from("sha256:");
    for byte in hasher.finalize() {
        write!(&mut content_id, "{byte:02x}")?;
    }
    Ok(content_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn demo_database() -> LibraryDb {
        let mut database = LibraryDb::in_memory().expect("in-memory baza mora biti dostupna");
        database
            .seed_demo_if_empty()
            .expect("probni podaci moraju biti uneseni");
        database
    }

    #[test]
    fn folder_navigation_returns_children_and_titles() {
        let database = demo_database();
        let root = database.root_items().unwrap();
        assert!(root.iter().any(|item| item.title == "Fantasy"));
        assert!(root.iter().any(|item| item.title == "City of Mist"));

        let fantasy = database.folder_items(1).unwrap();
        assert!(fantasy.iter().any(|item| item.title == "Epic sagas"));
        assert!(
            fantasy
                .iter()
                .any(|item| item.title == "Guardians of the Passage")
        );
    }

    #[test]
    fn title_can_belong_to_multiple_folders() {
        let database = demo_database();
        let fantasy = database.folder_items(1).unwrap();
        let graphic_novels = database.folder_items(2).unwrap();

        assert!(fantasy.iter().any(|item| item.id == "demo-cuvari-prolaza"));
        assert!(
            graphic_novels
                .iter()
                .any(|item| item.id == "demo-cuvari-prolaza")
        );
    }

    #[test]
    fn deleting_folder_removes_membership_but_not_title() {
        let database = demo_database();
        database.delete_folder(1).unwrap();

        assert!(database.title_exists("demo-cuvari-prolaza").unwrap());
        assert!(
            database
                .folder_items(2)
                .unwrap()
                .iter()
                .any(|item| item.id == "demo-cuvari-prolaza")
        );
    }

    #[test]
    fn removed_reading_item_returns_after_new_progress() {
        let database = demo_database();
        let title_id = "demo-posljednja-stanica";

        database.remove_from_reading(title_id).unwrap();
        assert!(
            !database
                .reading_items()
                .unwrap()
                .iter()
                .any(|item| item.id == title_id)
        );

        database
            .record_progress(title_id, "char:2200", 0.41)
            .unwrap();
        let reading = database.reading_items().unwrap();
        let item = reading.iter().find(|item| item.id == title_id).unwrap();
        assert_eq!(item.progress, Some(0.41));
    }

    #[test]
    fn favorite_toggle_is_persistent_in_queries() {
        let database = demo_database();
        let title_id = "demo-posljednja-stanica";

        database.toggle_favorite(title_id).unwrap();
        assert!(
            database
                .favorite_items()
                .unwrap()
                .iter()
                .any(|item| item.id == title_id)
        );

        database.toggle_favorite(title_id).unwrap();
        assert!(
            !database
                .favorite_items()
                .unwrap()
                .iter()
                .any(|item| item.id == title_id)
        );
    }

    #[test]
    fn bookmarks_toggle_and_keep_reading_order() {
        let database = demo_database();
        let title_id = "demo-grad-magla";

        assert!(
            database
                .toggle_bookmark(title_id, "spine:0:page:0", "Početak")
                .unwrap()
        );
        assert!(
            database
                .toggle_bookmark(title_id, "spine:2:page:1", "Poglavlje 2")
                .unwrap()
        );
        assert!(database.is_bookmarked(title_id, "spine:0:page:0").unwrap());
        assert_eq!(database.bookmarks(title_id).unwrap().len(), 2);

        assert!(
            !database
                .toggle_bookmark(title_id, "spine:0:page:0", "Početak")
                .unwrap()
        );
        assert_eq!(
            database.bookmarks(title_id).unwrap(),
            vec![Bookmark {
                locator: "spine:2:page:1".to_owned(),
                label: "Poglavlje 2".to_owned(),
            }]
        );
    }

    #[test]
    fn reader_preferences_are_validated_and_persisted() {
        let database = LibraryDb::in_memory().unwrap();
        assert_eq!(
            database.reader_preferences().unwrap(),
            ReaderPreferences::default()
        );

        let changed = ReaderPreferences {
            font_size: 22.0,
            theme: 1,
            margin: 24.0,
            line_spacing: 1.5,
        };
        database.set_reader_preferences(changed).unwrap();
        assert_eq!(database.reader_preferences().unwrap(), changed);

        assert!(
            database
                .set_reader_preferences(ReaderPreferences {
                    font_size: 40.0,
                    ..changed
                })
                .is_err()
        );
    }

    #[test]
    fn demo_seed_is_idempotent() {
        let mut database = demo_database();
        let before = database.root_items().unwrap();

        database.seed_demo_if_empty().unwrap();

        assert_eq!(database.root_items().unwrap(), before);
    }

    #[test]
    fn search_matches_title_or_author_case_insensitively() {
        let database = demo_database();

        assert_eq!(database.search_items("MIST").unwrap().len(), 1);
        assert_eq!(database.search_items("petrović").unwrap().len(), 1);
        assert!(database.search_items("ne postoji").unwrap().is_empty());
    }

    #[test]
    fn imported_file_uses_content_hash_and_skips_unchanged_rehash_result() {
        let mut database = LibraryDb::in_memory().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let book = temporary.path().join("Moja knjiga.TXT");
        fs::write(&book, "Probni sadržaj").unwrap();

        assert!(database.import_file(&book, None).unwrap());
        assert!(!database.import_file(&book, None).unwrap());

        let items = database.root_items().unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].id.starts_with("sha256:"));
        assert_eq!(items[0].title, "Moja knjiga");
        assert_eq!(items[0].format, "TXT");
    }

    #[test]
    fn android_import_uses_display_name_instead_of_internal_file_name() {
        let mut database = LibraryDb::in_memory().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let book = temporary.path().join("1791377112345-Knjiga1.epub");
        fs::write(&book, "probni sadržaj").unwrap();

        database
            .import_file_named(&book, None, Some("Knjiga1.epub"))
            .unwrap();

        assert_eq!(database.root_items().unwrap()[0].title, "Knjiga1");
        assert_eq!(
            cleaned_android_title("1791377112345-Strip1").as_deref(),
            Some("Strip1")
        );
        assert_eq!(
            cleaned_android_title("1791377112345Knjiga1").as_deref(),
            Some("Knjiga1")
        );
        assert_eq!(cleaned_android_title("1984-knjiga"), None);
    }

    #[test]
    fn android_reference_import_stores_uri_without_copying_document() {
        let mut database = LibraryDb::in_memory().unwrap();
        let uri = "content://provider/tree/library/document/strip.cbz";

        assert!(
            database
                .import_reference(uri, "Moj strip.cbz", None)
                .unwrap()
        );
        assert!(
            !database
                .import_reference(uri, "Moj strip.cbz", None)
                .unwrap()
        );

        let item = &database.root_items().unwrap()[0];
        assert_eq!(item.title, "Moj strip");
        let source = database.title_source(&item.id).unwrap().unwrap();
        assert_eq!(source.source, uri);
        assert!(source.id.starts_with("uri:"));
    }

    #[test]
    fn directory_import_preserves_folder_hierarchy_and_filters_files() {
        let mut database = LibraryDb::in_memory().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let collection = temporary.path().join("Kolekcija");
        let series = collection.join("Serijal");
        fs::create_dir_all(&series).unwrap();
        fs::write(collection.join("knjiga.epub"), "epub").unwrap();
        fs::write(series.join("strip.cbz"), "zip").unwrap();
        fs::write(series.join("slika.png"), "nije naslov").unwrap();

        let summary = database.import_directory(&collection, None).unwrap();
        assert_eq!(summary.imported, 2);
        assert_eq!(summary.skipped, 1);

        let root = database.root_items().unwrap();
        let collection_id = root
            .iter()
            .find(|item| item.title == "Kolekcija")
            .and_then(|item| folder_id_for_test(&item.id))
            .unwrap();
        let collection_items = database.folder_items(collection_id).unwrap();
        assert!(collection_items.iter().any(|item| item.title == "knjiga"));
        assert!(collection_items.iter().any(|item| item.title == "Serijal"));
    }

    #[test]
    fn root_directory_import_stays_next_to_existing_folder() {
        let mut database = LibraryDb::in_memory().unwrap();
        let existing_id = database.create_folder("Postojeći folder", None).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let imported = temporary.path().join("Novi folder");
        fs::create_dir(&imported).unwrap();
        fs::write(imported.join("strip.cbz"), "probni strip").unwrap();

        database.import_directory(&imported, None).unwrap();

        let root = database.root_items().unwrap();
        assert!(root.iter().any(|item| item.title == "Postojeći folder"));
        let imported_id = root
            .iter()
            .find(|item| item.title == "Novi folder")
            .and_then(|item| folder_id_for_test(&item.id))
            .unwrap();
        assert!(
            database
                .folder_items(imported_id)
                .unwrap()
                .iter()
                .any(|item| item.title == "strip")
        );
        assert!(database.folder_items(existing_id).unwrap().is_empty());
    }

    #[test]
    fn separate_android_imports_never_merge_root_folders() {
        let mut database = LibraryDb::in_memory().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let first = temporary.path().join("android-batch-1");
        let second = temporary.path().join("android-batch-2");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        fs::write(first.join("prvi.cbz"), "prvi strip").unwrap();
        fs::write(second.join("drugi.cbz"), "drugi strip").unwrap();

        database
            .import_directory_as_new(&first, None, "Strip-folder")
            .unwrap();
        database
            .import_directory_as_new(&second, None, "Strip-folder")
            .unwrap();

        let root = database.root_items().unwrap();
        assert!(root.iter().any(|item| item.title == "Strip-folder"));
        assert!(root.iter().any(|item| item.title == "Strip-folder (2)"));
        let first_id = root
            .iter()
            .find(|item| item.title == "Strip-folder")
            .and_then(|item| folder_id_for_test(&item.id))
            .unwrap();
        let second_id = root
            .iter()
            .find(|item| item.title == "Strip-folder (2)")
            .and_then(|item| folder_id_for_test(&item.id))
            .unwrap();
        assert_eq!(database.folder_items(first_id).unwrap()[0].title, "prvi");
        assert_eq!(database.folder_items(second_id).unwrap()[0].title, "drugi");
    }

    #[test]
    fn removing_imported_title_never_deletes_source_file() {
        let mut database = LibraryDb::in_memory().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let book = temporary.path().join("ostaje.txt");
        fs::write(&book, "Sadržaj koji ostaje na disku").unwrap();
        database.import_file(&book, None).unwrap();
        let title_id = database.root_items().unwrap()[0].id.clone();

        database.remove_title_from_library(&title_id).unwrap();

        assert!(book.exists());
        assert!(!database.title_exists(&title_id).unwrap());
    }

    #[test]
    fn same_title_stays_in_root_when_added_to_folder() {
        let mut database = LibraryDb::in_memory().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let comic = temporary.path().join("strip1.cbz");
        fs::write(&comic, "isti sadržaj stripa").unwrap();
        let folder_id = database.create_folder("Stripovi", None).unwrap();

        assert!(database.import_file(&comic, None).unwrap());
        assert!(!database.import_file(&comic, Some(folder_id)).unwrap());

        assert!(
            database
                .root_items()
                .unwrap()
                .iter()
                .any(|item| item.title == "strip1")
        );
        assert!(
            database
                .folder_items(folder_id)
                .unwrap()
                .iter()
                .any(|item| item.title == "strip1")
        );
    }

    #[test]
    fn removing_root_location_keeps_folder_location() {
        let mut database = LibraryDb::in_memory().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let comic = temporary.path().join("strip1.cbz");
        fs::write(&comic, "isti sadržaj stripa").unwrap();
        let folder_id = database.create_folder("Stripovi", None).unwrap();
        database.import_file(&comic, None).unwrap();
        database.import_file(&comic, Some(folder_id)).unwrap();
        let title_id = database
            .folder_items(folder_id)
            .unwrap()
            .into_iter()
            .find(|item| item.title == "strip1")
            .unwrap()
            .id;

        database.remove_title_from_library(&title_id).unwrap();

        assert!(
            !database
                .root_items()
                .unwrap()
                .iter()
                .any(|item| item.id == title_id)
        );
        assert!(
            database
                .folder_items(folder_id)
                .unwrap()
                .iter()
                .any(|item| item.id == title_id)
        );
        assert!(comic.exists());
    }

    fn folder_id_for_test(value: &str) -> Option<i64> {
        value.strip_prefix("folder:")?.parse().ok()
    }
}
