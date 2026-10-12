mod library;
#[cfg(citac_qt_epub)]
mod qt_epub;
mod reader;
mod thumbnail;

use std::{
    cell::RefCell,
    collections::HashSet,
    error::Error,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

#[cfg(target_os = "android")]
use std::collections::HashMap;

#[cfg(not(target_os = "android"))]
use directories::ProjectDirs;
use library::{ItemKind, LibraryDb, LibraryItem};
use reader::DocumentPage;
use slint::{Color, Image, Model, ModelRc, SharedString, VecModel};

#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::OnceLock<slint::android::AndroidApp> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
fn android_open_documents() {
    let Some(android_app) = ANDROID_APP.get().cloned() else {
        return;
    };
    let callback_app = android_app.clone();
    android_app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { jni::JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let _ = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let raw_activity = callback_app.activity_as_ptr() as jni::sys::jobject;
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&raw_activity)? };
            env.call_method(
                activity,
                jni::jni_str!("openDocuments"),
                jni::jni_sig!("()V"),
                &[],
            )?;
            Ok(())
        });
    }));
}

#[cfg(target_os = "android")]
fn android_open_folder() {
    let Some(android_app) = ANDROID_APP.get().cloned() else {
        return;
    };
    let callback_app = android_app.clone();
    android_app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { jni::JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let _ = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let raw_activity = callback_app.activity_as_ptr() as jni::sys::jobject;
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&raw_activity)? };
            env.call_method(
                activity,
                jni::jni_str!("openFolder"),
                jni::jni_sig!("()V"),
                &[],
            )?;
            Ok(())
        });
    }));
}

slint::include_modules!();

#[derive(Clone)]
struct ReaderSession {
    source: library::TitleSource,
    pages: Vec<DocumentPage>,
    navigation_pages: Vec<DocumentPage>,
    backend: ReaderBackend,
    cancelled: Arc<AtomicBool>,
    resolved_source: ResolvedDocument,
}

#[derive(Clone)]
struct ResolvedDocument {
    path: PathBuf,
    #[cfg(target_os = "android")]
    _handle: Arc<AndroidDocumentHandle>,
}

#[cfg(target_os = "android")]
struct AndroidDocumentHandle {
    fd: i32,
}

#[cfg(target_os = "android")]
impl Drop for AndroidDocumentHandle {
    fn drop(&mut self) {
        let Some(android_app) = ANDROID_APP.get().cloned() else {
            return;
        };
        let callback_app = android_app.clone();
        let fd = self.fd;
        let _ = android_app.run_on_java_main_thread(Box::new(move || {
            let vm = unsafe { jni::JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
            let _ = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
                let raw_activity = callback_app.activity_as_ptr() as jni::sys::jobject;
                let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&raw_activity)? };
                env.call_method(
                    activity,
                    jni::jni_str!("closeDocumentFd"),
                    jni::jni_sig!((fd: int) -> void),
                    &[jni::objects::JValue::Int(fd)],
                )?;
                Ok(())
            });
        }));
    }
}

fn resolve_document(source: &str) -> reader::ReaderResult<ResolvedDocument> {
    #[cfg(target_os = "android")]
    if source.starts_with("content://") {
        let android_app = ANDROID_APP.get().cloned().ok_or("Android is not ready")?;
        let vm = unsafe { jni::JavaVM::from_raw(android_app.vm_as_ptr().cast()) };
        let fd = vm.attach_current_thread(|env| -> jni::errors::Result<i32> {
            let raw_activity = android_app.activity_as_ptr() as jni::sys::jobject;
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&raw_activity)? };
            let uri = env.new_string(source)?;
            env.call_method(
                activity,
                jni::jni_str!("openDocumentFd"),
                jni::jni_sig!((uri: JString) -> int),
                &[jni::objects::JValue::Object(&uri)],
            )?
            .i()
        })?;
        if fd < 0 {
            return Err("Android could not open the original document".into());
        }
        return Ok(ResolvedDocument {
            path: PathBuf::from(format!("/proc/self/fd/{fd}")),
            _handle: Arc::new(AndroidDocumentHandle { fd }),
        });
    }

    Ok(ResolvedDocument {
        path: PathBuf::from(source),
        #[cfg(target_os = "android")]
        _handle: Arc::new(AndroidDocumentHandle { fd: -1 }),
    })
}

#[derive(Clone, Default)]
enum ReaderBackend {
    #[default]
    Builtin,
    Comic(Arc<reader::ComicRenderCache>),
    #[cfg(citac_qt_epub)]
    Qt(Arc<qt_epub::QtEpubSession>),
}

impl ReaderBackend {
    fn render_page(
        &self,
        source: &library::TitleSource,
        path: &Path,
        page: &DocumentPage,
        _page_index: usize,
    ) -> reader::ReaderResult<Vec<reader::RenderedBlock>> {
        match self {
            Self::Builtin => reader::render_page(path, &source.format, page),
            Self::Comic(cache) => cache.render_page(_page_index),
            #[cfg(citac_qt_epub)]
            Self::Qt(session) => session.render_page(_page_index),
        }
    }

    fn preload_neighbors(&self, page_index: usize) {
        if let Self::Comic(cache) = self {
            cache.preload_neighbors(page_index);
        }
        #[cfg(citac_qt_epub)]
        if let Self::Qt(session) = self {
            session.preload_neighbors(page_index);
        }
        #[cfg(not(citac_qt_epub))]
        let _ = page_index;
    }
}

fn ui_entry(item: LibraryItem, selected: bool) -> LibraryEntry {
    let accent = match item.kind {
        ItemKind::Folder => Color::from_rgb_u8(71, 89, 168),
        ItemKind::Book => Color::from_rgb_u8(65, 111, 101),
        ItemKind::Comic => Color::from_rgb_u8(176, 100, 63),
    };
    let badge = match item.progress {
        Some(progress) => format!("{} · {:.0}% read", item.format, progress * 100.0),
        None => match item.kind {
            ItemKind::Folder => item.format.clone(),
            _ => format!("{} · not started", item.format),
        },
    };

    let cover = item
        .thumbnail_path
        .as_deref()
        .filter(|path| !path.is_empty())
        .and_then(|path| Image::load_from_path(Path::new(path)).ok());
    let has_cover = cover.is_some();

    LibraryEntry {
        entry_id: SharedString::from(item.id),
        title: SharedString::from(item.title),
        subtitle: SharedString::from(item.subtitle),
        badge: SharedString::from(badge),
        accent,
        entry_kind: match item.kind {
            ItemKind::Folder => 0,
            ItemKind::Book => 1,
            ItemKind::Comic => 2,
        },
        favorite: item.favorite,
        selected,
        cover: cover.unwrap_or_default(),
        has_cover,
    }
}

fn library_rows(entries: Vec<LibraryEntry>) -> ModelRc<LibraryRow> {
    let mut rows = Vec::with_capacity(entries.len().div_ceil(2));
    for pair in entries.chunks(2) {
        rows.push(LibraryRow {
            first: pair[0].clone(),
            second: pair.get(1).cloned().unwrap_or_else(|| pair[0].clone()),
            has_second: pair.len() == 2,
        });
    }
    ModelRc::new(VecModel::from(rows))
}

fn ui_model(items: Vec<LibraryItem>) -> ModelRc<LibraryRow> {
    library_rows(
        items
            .into_iter()
            .map(|item| ui_entry(item, false))
            .collect(),
    )
}

fn selected_ui_model(items: Vec<LibraryItem>, selection: &HashSet<String>) -> ModelRc<LibraryRow> {
    library_rows(
        items
            .into_iter()
            .map(|item| {
                let selected = selection.contains(&item.id);
                ui_entry(item, selected)
            })
            .collect::<Vec<_>>(),
    )
}

fn folder_id(value: &str) -> Option<i64> {
    value.strip_prefix("folder:")?.parse().ok()
}

fn refresh_models(
    app: &AppWindow,
    database: &LibraryDb,
    current_folder: Option<i64>,
) -> rusqlite::Result<()> {
    app.set_library_items(ui_model(database.root_items()?));
    app.set_favorite_items(ui_model(database.favorite_items()?));
    app.set_reading_items(ui_model(database.reading_items()?));
    if let Some(folder_id) = current_folder {
        app.set_folder_items(ui_model(database.folder_items(folder_id)?));
    }
    Ok(())
}

fn refresh_library_context(
    app: &AppWindow,
    database: &LibraryDb,
    current_folder: Option<i64>,
    selection: &HashSet<String>,
) -> rusqlite::Result<()> {
    if let Some(folder_id) = current_folder {
        app.set_folder_items(selected_ui_model(
            database.folder_items(folder_id)?,
            selection,
        ));
    } else {
        app.set_library_items(selected_ui_model(database.root_items()?, selection));
    }
    Ok(())
}

fn finish_background_import(weak_app: slint::Weak<AppWindow>, message: String) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak_app.upgrade() {
            app.invoke_import_completed(message.into());
        }
    });
}

fn generate_pending_thumbnails(
    database: &LibraryDb,
    cache_directory: &Path,
) -> Result<usize, Box<dyn Error + Send + Sync>> {
    let candidates = database.thumbnail_candidates()?;
    let mut generated = 0;
    for candidate in candidates {
        let resolved = resolve_document(&candidate.source).ok();
        let thumbnail = resolved
            .as_ref()
            .and_then(|source| thumbnail::generate(&candidate, &source.path, cache_directory));
        if thumbnail.is_some() {
            generated += 1;
        }
        database.set_thumbnail_path(
            &candidate.id,
            Some(thumbnail.as_deref().unwrap_or_else(|| Path::new(""))),
        )?;
    }
    Ok(generated)
}

#[cfg(target_os = "android")]
fn import_saf_reference_batch(
    database: &mut LibraryDb,
    batch: &Path,
) -> Result<usize, Box<dyn Error + Send + Sync>> {
    let root_name = fs::read_to_string(batch.join(".root-name"))
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    let root_folder = root_name
        .as_deref()
        .map(|name| database.create_unique_root_folder(name))
        .transpose()?;
    let mut folders = HashMap::<String, i64>::new();
    if let Some(root) = root_folder {
        folders.insert(String::new(), root);
    }
    let mut references = fs::read_dir(batch)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("ref"))
        .collect::<Vec<_>>();
    references.sort();
    let mut imported = 0;
    for reference in references {
        let contents = fs::read_to_string(reference)?;
        let mut lines = contents.lines();
        let Some(uri) = lines
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(name) = lines
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let relative = lines.next().unwrap_or("").trim_matches('/');
        let folder_id = if let Some(root) = root_folder {
            let mut parent = root;
            let mut accumulated = String::new();
            for component in Path::new(relative).components() {
                let std::path::Component::Normal(component) = component else {
                    continue;
                };
                let component = component.to_string_lossy();
                if !accumulated.is_empty() {
                    accumulated.push('/');
                }
                accumulated.push_str(&component);
                parent = if let Some(existing) = folders.get(&accumulated) {
                    *existing
                } else {
                    let created = database.create_folder(&component, Some(parent))?;
                    folders.insert(accumulated.clone(), created);
                    created
                };
            }
            Some(parent)
        } else {
            None
        };
        if database.import_reference(uri, name, folder_id)? {
            imported += 1;
        }
    }
    Ok(imported)
}

fn reader_ui_model(pages: Vec<DocumentPage>) -> ModelRc<ReaderPage> {
    ModelRc::new(VecModel::from(
        pages
            .into_iter()
            .map(|page| ReaderPage {
                heading: page.heading.into(),
                body: page.body.into(),
                locator: page.locator.into(),
                progress: page.progress,
            })
            .collect::<Vec<_>>(),
    ))
}

fn reader_blocks_ui_model(blocks: Vec<reader::RenderedBlock>) -> ModelRc<ReaderBlock> {
    ModelRc::new(VecModel::from(
        blocks
            .into_iter()
            .map(|block| match block {
                reader::RenderedBlock::Text(text) => ReaderBlock {
                    text: text.into(),
                    picture: Image::default(),
                    is_image: false,
                    aspect_ratio: 1.0,
                    pixel_width: 0.0,
                    pixel_height: 0.0,
                },
                reader::RenderedBlock::Image {
                    pixels,
                    premultiplied,
                } => {
                    let pixel_width = pixels.width() as f32;
                    let pixel_height = pixels.height() as f32;
                    let aspect_ratio = pixel_width / pixel_height.max(1.0);
                    let picture = if premultiplied {
                        Image::from_rgba8_premultiplied(pixels)
                    } else {
                        Image::from_rgba8(pixels)
                    };
                    ReaderBlock {
                        text: SharedString::new(),
                        picture,
                        is_image: true,
                        aspect_ratio,
                        pixel_width,
                        pixel_height,
                    }
                }
            })
            .collect::<Vec<_>>(),
    ))
}

fn reader_tools_ui_model(items: Vec<ReaderToolItem>) -> ModelRc<ReaderToolItem> {
    ModelRc::new(VecModel::from(items))
}

fn mapped_reader_index(page: &DocumentPage, source_index: usize, target_count: usize) -> i32 {
    if target_count == 0 {
        return 0;
    }
    if page.progress.is_finite() {
        (page.progress.clamp(0.0, 1.0) * (target_count.saturating_sub(1)) as f32).round() as i32
    } else {
        source_index.min(target_count - 1) as i32
    }
}

fn reader_toc_items(pages: &[DocumentPage], target_count: usize) -> Vec<ReaderToolItem> {
    pages
        .iter()
        .enumerate()
        .filter(|(_, page)| !page.heading.trim().is_empty())
        .map(|(index, page)| ReaderToolItem {
            label: page.heading.clone().into(),
            detail: format!("Page {}", index + 1).into(),
            page_index: mapped_reader_index(page, index, target_count),
        })
        .collect()
}

fn reader_search_items(
    pages: &[DocumentPage],
    target_count: usize,
    query: &str,
) -> Vec<ReaderToolItem> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    pages
        .iter()
        .enumerate()
        .filter(|(_, page)| {
            page.heading.to_lowercase().contains(&query)
                || page.body.to_lowercase().contains(&query)
        })
        .take(100)
        .map(|(index, page)| {
            let summary = page
                .body
                .split_whitespace()
                .take(18)
                .collect::<Vec<_>>()
                .join(" ");
            ReaderToolItem {
                label: if page.heading.trim().is_empty() {
                    format!("Page {}", index + 1).into()
                } else {
                    page.heading.clone().into()
                },
                detail: summary.into(),
                page_index: mapped_reader_index(page, index, target_count),
            }
        })
        .collect()
}

fn reader_bookmark_items(
    bookmarks: Vec<library::Bookmark>,
    pages: &[DocumentPage],
) -> Vec<ReaderToolItem> {
    bookmarks
        .into_iter()
        .filter_map(|bookmark| {
            let index = pages
                .iter()
                .position(|page| page.locator == bookmark.locator)?;
            Some(ReaderToolItem {
                label: bookmark.label.into(),
                detail: format!("Page {}", index + 1).into(),
                page_index: index as i32,
            })
        })
        .collect()
}

fn refresh_reader_bookmarks(
    app: &AppWindow,
    database: &LibraryDb,
    title_id: &str,
    pages: &[DocumentPage],
    current_locator: &str,
) -> rusqlite::Result<()> {
    app.set_reader_bookmark_items(reader_tools_ui_model(reader_bookmark_items(
        database.bookmarks(title_id)?,
        pages,
    )));
    app.set_reader_current_bookmarked(database.is_bookmarked(title_id, current_locator)?);
    Ok(())
}

fn load_reader_in_background(
    weak_app: slint::Weak<AppWindow>,
    source: library::TitleSource,
    session: Arc<Mutex<Option<ReaderSession>>>,
    cancelled: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let resolved_source = match resolve_document(&source.source) {
            Ok(resolved) => resolved,
            Err(error) => {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak_app.upgrade() {
                        app.set_reader_loading(false);
                        app.set_reader_message(format!("Could not open document: {error}").into());
                    }
                });
                return;
            }
        };
        let path = resolved_source.path.clone();
        let result: reader::ReaderResult<_> = (|| {
            if source.format == "EPUB" {
                #[cfg(citac_qt_epub)]
                {
                    let qt = Arc::new(qt_epub::QtEpubSession::open(&path)?);
                    let pages = qt.pages();
                    let navigation_pages = reader::load_document(&path, "EPUB")?.pages;
                    let index = source
                        .progress_locator
                        .as_deref()
                        .and_then(|locator| locator.strip_prefix("qt-page:"))
                        .and_then(|value| value.parse::<usize>().ok())
                        .filter(|index| *index < pages.len())
                        .unwrap_or(0);
                    let backend = ReaderBackend::Qt(qt);
                    let blocks = backend.render_page(&source, &path, &pages[index], index)?;
                    return Ok((
                        pages,
                        index,
                        blocks,
                        Vec::new(),
                        backend,
                        true,
                        navigation_pages,
                    ));
                }
                #[cfg(all(not(citac_qt_epub), target_os = "linux"))]
                return Err(
                    "This build does not include the Qt EPUB renderer. Build it with the default features."
                        .into(),
                );
            }

            let document = reader::load_document(&path, &source.format)?;
            let pages = document.pages;
            let index = source
                .progress_locator
                .as_deref()
                .and_then(|locator| pages.iter().position(|page| page.locator == locator))
                .unwrap_or(0);
            let backend = if matches!(source.format.as_str(), "CBZ" | "CBR") {
                ReaderBackend::Comic(Arc::new(reader::ComicRenderCache::new(
                    path.clone(),
                    source.format.clone(),
                    pages.clone(),
                )))
            } else {
                ReaderBackend::Builtin
            };
            let blocks = backend.render_page(&source, &path, &pages[index], index)?;
            let secondary_blocks = if matches!(source.format.as_str(), "CBZ" | "CBR") {
                pages
                    .get(index + 1)
                    .map(|page| backend.render_page(&source, &path, page, index + 1))
                    .transpose()?
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let fixed_page = matches!(source.format.as_str(), "CBZ" | "CBR")
                || (source.format == "EPUB"
                    && blocks.len() == 1
                    && matches!(blocks.first(), Some(reader::RenderedBlock::Image { .. })));
            let navigation_pages = pages.clone();
            Ok((
                pages,
                index,
                blocks,
                secondary_blocks,
                backend,
                fixed_page,
                navigation_pages,
            ))
        })();
        let preload = result
            .as_ref()
            .ok()
            .map(|(_, index, _, _, backend, _, _)| (backend.clone(), *index));
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let session_cancelled = Arc::clone(&cancelled);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            if app.get_view_mode() != 2 || app.get_reader_title_id().as_str() != source.id {
                return;
            }
            match result {
                Ok((
                    pages,
                    index,
                    blocks,
                    secondary_blocks,
                    backend,
                    fixed_page,
                    navigation_pages,
                )) => {
                    let current = pages.get(index).cloned();
                    app.set_reader_toc_items(reader_tools_ui_model(reader_toc_items(
                        &navigation_pages,
                        pages.len(),
                    )));
                    app.set_reader_search_items(reader_tools_ui_model(Vec::new()));
                    app.set_reader_bookmark_items(reader_tools_ui_model(Vec::new()));
                    app.set_reader_current_bookmarked(false);
                    if let Ok(mut active) = session.lock() {
                        *active = Some(ReaderSession {
                            source: source.clone(),
                            pages: pages.clone(),
                            navigation_pages,
                            backend,
                            cancelled: session_cancelled,
                            resolved_source,
                        });
                    }
                    app.set_reader_pages(reader_ui_model(pages));
                    app.set_reader_blocks(reader_blocks_ui_model(blocks));
                    app.set_reader_secondary_blocks(reader_blocks_ui_model(secondary_blocks));
                    app.set_reader_fixed_page(fixed_page);
                    app.set_reader_is_comic(matches!(source.format.as_str(), "CBZ" | "CBR"));
                    app.set_reader_index(index as i32);
                    app.set_reader_loading(false);
                    app.set_reader_message(String::new().into());
                    if let Some(page) = current {
                        app.invoke_reader_page_changed(
                            source.id.into(),
                            page.locator.into(),
                            page.progress,
                        );
                    }
                }
                Err(error) => {
                    app.set_reader_pages(reader_ui_model(Vec::new()));
                    app.set_reader_blocks(reader_blocks_ui_model(Vec::new()));
                    app.set_reader_secondary_blocks(reader_blocks_ui_model(Vec::new()));
                    app.set_reader_loading(false);
                    app.set_reader_message(format!("Could not open content: {error}").into());
                }
            }
        });
        if !cancelled.load(Ordering::Acquire)
            && let Some((backend, index)) = preload
        {
            backend.preload_neighbors(index);
        }
    });
}

fn load_reader_page_in_background(
    weak_app: slint::Weak<AppWindow>,
    session: Arc<Mutex<Option<ReaderSession>>>,
    index: usize,
) {
    thread::spawn(move || {
        let active = session.lock().ok().and_then(|session| session.clone());
        let Some(active) = active else {
            return;
        };
        if active.cancelled.load(Ordering::Acquire) {
            return;
        }
        let Some(page) = active.pages.get(index).cloned() else {
            return;
        };
        let title_id = active.source.id.clone();
        let result = (|| {
            let blocks = active.backend.render_page(
                &active.source,
                &active.resolved_source.path,
                &page,
                index,
            )?;
            let secondary_blocks = if matches!(active.source.format.as_str(), "CBZ" | "CBR") {
                active
                    .pages
                    .get(index + 1)
                    .map(|next| {
                        active.backend.render_page(
                            &active.source,
                            &active.resolved_source.path,
                            next,
                            index + 1,
                        )
                    })
                    .transpose()?
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            Ok::<_, Box<dyn Error + Send + Sync>>((blocks, secondary_blocks))
        })();
        if active.cancelled.load(Ordering::Acquire) {
            return;
        }
        let preload_backend = active.backend.clone();
        let preload_cancelled = Arc::clone(&active.cancelled);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            if app.get_reader_title_id().as_str() != title_id {
                return;
            }
            if app.get_reader_index() != index as i32 {
                return;
            }
            match result {
                Ok((blocks, secondary_blocks)) => {
                    app.set_reader_blocks(reader_blocks_ui_model(blocks));
                    app.set_reader_secondary_blocks(reader_blocks_ui_model(secondary_blocks));
                    app.set_reader_loading(false);
                    app.set_reader_message(String::new().into());
                    app.invoke_reader_page_changed(
                        title_id.into(),
                        page.locator.into(),
                        page.progress,
                    );
                }
                Err(error) => {
                    app.set_reader_loading(false);
                    app.set_reader_message(format!("Could not open page: {error}").into());
                }
            }
        });
        if !preload_cancelled.load(Ordering::Acquire) {
            preload_backend.preload_neighbors(index);
        }
    });
}

#[cfg(not(target_os = "android"))]
fn import_files_in_background(
    weak_app: slint::Weak<AppWindow>,
    database_path: PathBuf,
    thumbnail_directory: PathBuf,
    folder_id: Option<i64>,
) {
    thread::spawn(move || {
        let files = rfd::FileDialog::new()
            .set_title("Add books or comics")
            .add_filter("Supported formats", &["epub", "cbr", "cbz", "txt"])
            .pick_files();
        let Some(files) = files else {
            finish_background_import(weak_app, "Adding files was cancelled.".to_owned());
            return;
        };

        let result = (|| -> library::ImportResult<(usize, usize)> {
            let mut database = LibraryDb::open(&database_path)?;
            let mut imported = 0;
            let mut unchanged = 0;
            for file in files {
                if database.import_file(&file, folder_id)? {
                    imported += 1;
                } else {
                    unchanged += 1;
                }
            }
            generate_pending_thumbnails(&database, &thumbnail_directory)?;
            Ok((imported, unchanged))
        })();

        let message = match result {
            Ok((imported, unchanged)) => {
                format!("Import complete: {imported} new, {unchanged} unchanged.")
            }
            Err(error) => format!("Import failed: {error}"),
        };
        finish_background_import(weak_app, message);
    });
}

#[cfg(not(target_os = "android"))]
fn import_folder_in_background(
    weak_app: slint::Weak<AppWindow>,
    database_path: PathBuf,
    thumbnail_directory: PathBuf,
    parent_id: Option<i64>,
) {
    thread::spawn(move || {
        let directory = rfd::FileDialog::new()
            .set_title("Import a folder with books and comics")
            .pick_folder();
        let Some(directory) = directory else {
            finish_background_import(weak_app, "Folder import was cancelled.".to_owned());
            return;
        };

        let result = (|| -> library::ImportResult<library::ImportSummary> {
            let mut database = LibraryDb::open(&database_path)?;
            let summary = database.import_directory(&directory, parent_id)?;
            generate_pending_thumbnails(&database, &thumbnail_directory)?;
            Ok(summary)
        })();
        let message = match result {
            Ok(summary) => format!(
                "Import complete: {} new, {} unchanged, {} skipped.",
                summary.imported, summary.unchanged, summary.skipped
            ),
            Err(error) => format!("Folder import failed: {error}"),
        };
        finish_background_import(weak_app, message);
    });
}

fn run_app(data_directory: PathBuf) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(&data_directory)?;
    let database_path = data_directory.join("biblioteka.sqlite3");
    let thumbnail_directory = data_directory.join("thumbnails");
    let mut database = LibraryDb::open(&database_path)?;
    database.seed_demo_if_empty()?;

    let app = AppWindow::new()?;
    app.set_app_version(env!("CARGO_PKG_VERSION").into());
    let preferences = database.reader_preferences()?;
    app.set_reader_font_size(preferences.font_size);
    app.set_reader_theme(preferences.theme);
    app.set_reader_margin(preferences.margin);
    app.set_reader_line_spacing(preferences.line_spacing);
    let mobile_preview = std::env::var_os("CITAC_MOBILE_PREVIEW").is_some();
    app.set_mobile_mode(cfg!(target_os = "android") || mobile_preview);
    if mobile_preview {
        app.window().set_size(slint::PhysicalSize::new(360, 800));
    }
    refresh_models(&app, &database, None)?;

    let database = Rc::new(RefCell::new(database));
    let folder_stack = Rc::new(RefCell::new(Vec::<i64>::new()));
    let selection = Rc::new(RefCell::new(HashSet::<String>::new()));
    let reader_session = Arc::new(Mutex::new(None::<ReaderSession>));
    let reader_task_cancel = Arc::new(Mutex::new(None::<Arc<AtomicBool>>));

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let reader_session = Arc::clone(&reader_session);
        let reader_task_cancel = Arc::clone(&reader_task_cancel);
        app.on_open_title_requested(move |title_id| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            match database.borrow().title_source(title_id.as_str()) {
                Ok(Some(source)) => {
                    let cancelled = Arc::new(AtomicBool::new(false));
                    if let Ok(mut current) = reader_task_cancel.lock() {
                        if let Some(previous) = current.replace(Arc::clone(&cancelled)) {
                            previous.store(true, Ordering::Release);
                        }
                    }
                    load_reader_in_background(
                        app.as_weak(),
                        source,
                        Arc::clone(&reader_session),
                        cancelled,
                    )
                }
                Ok(None) => {
                    app.set_reader_loading(false);
                    app.set_reader_message("This title is no longer in the library.".into());
                }
                Err(error) => {
                    app.set_reader_loading(false);
                    app.set_reader_message(format!("Could not open title: {error}").into());
                }
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        app.on_reader_preferences_changed(move |font_size, theme, margin, line_spacing| {
            if let Err(error) =
                database
                    .borrow()
                    .set_reader_preferences(library::ReaderPreferences {
                        font_size,
                        theme,
                        margin,
                        line_spacing,
                    })
                && let Some(app) = weak_app.upgrade()
            {
                app.set_reader_message(format!("Settings could not be saved: {error}").into());
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let reader_session = Arc::clone(&reader_session);
        app.on_reader_page_selected(move |index| {
            if index < 0 {
                return;
            }
            load_reader_page_in_background(
                weak_app.clone(),
                Arc::clone(&reader_session),
                index as usize,
            );
        });
    }

    {
        let weak_app = app.as_weak();
        let reader_session = Arc::clone(&reader_session);
        app.on_reader_page_number_requested(move |value| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let page_count = app.get_reader_pages().row_count();
            let requested = value.trim().parse::<usize>().ok();
            let Some(index) = requested
                .filter(|page| *page > 0 && *page <= page_count)
                .map(|page| page - 1)
            else {
                app.set_reader_message(format!("Unesi broj od 1 do {page_count}.").into());
                return;
            };
            if index == app.get_reader_index() as usize {
                return;
            }
            app.set_reader_index(index as i32);
            app.set_reader_loading(true);
            app.set_reader_message("Loading page…".into());
            load_reader_page_in_background(app.as_weak(), Arc::clone(&reader_session), index);
        });
    }

    {
        let database = Rc::clone(&database);
        let reader_session = Arc::clone(&reader_session);
        let weak_app = app.as_weak();
        app.on_reader_page_changed(move |title_id, locator, progress| {
            let database = database.borrow();
            if let Err(error) =
                database.record_progress(title_id.as_str(), locator.as_str(), f64::from(progress))
            {
                eprintln!("Could not save reading position: {error}");
                return;
            }
            if let Some(app) = weak_app.upgrade() {
                if let Ok(items) = database.reading_items() {
                    app.set_reading_items(ui_model(items));
                }
                if let Some(active) = reader_session
                    .lock()
                    .ok()
                    .and_then(|session| session.clone())
                {
                    let _ = refresh_reader_bookmarks(
                        &app,
                        &database,
                        title_id.as_str(),
                        &active.pages,
                        locator.as_str(),
                    );
                }
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let reader_session = Arc::clone(&reader_session);
        app.on_reader_search_requested(move |query| {
            let query = query.to_string();
            let active = reader_session
                .lock()
                .ok()
                .and_then(|session| session.clone());
            let Some(active) = active else {
                return;
            };
            let title_id = active.source.id.clone();
            let result_app = weak_app.clone();
            thread::spawn(move || {
                let items =
                    reader_search_items(&active.navigation_pages, active.pages.len(), &query);
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = result_app.upgrade() else {
                        return;
                    };
                    if app.get_reader_title_id().as_str() == title_id
                        && app.get_reader_search_query().as_str() == query
                    {
                        app.set_reader_search_items(reader_tools_ui_model(items));
                    }
                });
            });
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let reader_session = Arc::clone(&reader_session);
        app.on_reader_bookmark_toggled(move |title_id, locator, label| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let active = reader_session
                .lock()
                .ok()
                .and_then(|session| session.clone());
            let Some(active) = active else {
                return;
            };
            let database = database.borrow();
            if let Err(error) = database
                .toggle_bookmark(title_id.as_str(), locator.as_str(), label.as_str())
                .and_then(|_| {
                    refresh_reader_bookmarks(
                        &app,
                        &database,
                        title_id.as_str(),
                        &active.pages,
                        locator.as_str(),
                    )
                })
            {
                app.set_reader_message(format!("Bookmark could not be saved: {error}").into());
            }
        });
    }

    {
        let reader_session = Arc::clone(&reader_session);
        let reader_task_cancel = Arc::clone(&reader_task_cancel);
        let weak_app = app.as_weak();
        app.on_reader_closed(move || {
            if let Ok(mut current) = reader_task_cancel.lock()
                && let Some(cancelled) = current.take()
            {
                cancelled.store(true, Ordering::Release);
            }
            if let Ok(mut active) = reader_session.lock() {
                *active = None;
            }
            if let Some(app) = weak_app.upgrade() {
                app.set_reader_title_id(String::new().into());
                app.set_reader_fixed_page(false);
                app.set_reader_is_comic(false);
                app.set_reader_fullscreen(false);
                app.set_reader_controls_visible(true);
                app.window().set_fullscreen(false);
                app.set_reader_pages(reader_ui_model(Vec::new()));
                app.set_reader_blocks(reader_blocks_ui_model(Vec::new()));
                app.set_reader_secondary_blocks(reader_blocks_ui_model(Vec::new()));
                app.set_reader_toc_items(reader_tools_ui_model(Vec::new()));
                app.set_reader_search_items(reader_tools_ui_model(Vec::new()));
                app.set_reader_bookmark_items(reader_tools_ui_model(Vec::new()));
                app.set_reader_panel(0);
                app.set_reader_search_query(String::new().into());
            }
        });
    }

    {
        let weak_app = app.as_weak();
        app.on_reader_fullscreen_toggled(move |enabled| {
            if let Some(app) = weak_app.upgrade() {
                app.window().set_fullscreen(enabled);
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        app.on_import_completed(move |message| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let current_folder = folder_stack.borrow().last().copied();
            let database = database.borrow();
            if let Err(error) = refresh_models(&app, &database, current_folder) {
                app.set_status_message(format!("Refresh failed: {error}").into());
            } else {
                app.set_status_message(message);
            }
            app.set_busy(false);
        });
    }

    {
        #[cfg(not(target_os = "android"))]
        let weak_app = app.as_weak();
        let callback_app = app.as_weak();
        #[cfg(not(target_os = "android"))]
        let database_path = database_path.clone();
        #[cfg(not(target_os = "android"))]
        let thumbnail_directory = thumbnail_directory.clone();
        #[cfg(not(target_os = "android"))]
        let folder_stack = Rc::clone(&folder_stack);
        app.on_add_files_requested(move || {
            let Some(app) = callback_app.upgrade() else {
                return;
            };
            #[cfg(target_os = "android")]
            {
                android_open_documents();
                app.set_status_message("Select books or comics in the Android dialog.".into());
                return;
            }
            #[cfg(not(target_os = "android"))]
            {
                app.set_busy(true);
                app.set_status_message("Selecting and importing files…".into());
                let folder_id = folder_stack.borrow().last().copied();
                import_files_in_background(
                    weak_app.clone(),
                    database_path.clone(),
                    thumbnail_directory.clone(),
                    folder_id,
                );
            }
        });
    }

    {
        #[cfg(not(target_os = "android"))]
        let weak_app = app.as_weak();
        let callback_app = app.as_weak();
        #[cfg(not(target_os = "android"))]
        let database_path = database_path.clone();
        #[cfg(not(target_os = "android"))]
        let thumbnail_directory = thumbnail_directory.clone();
        #[cfg(not(target_os = "android"))]
        let folder_stack = Rc::clone(&folder_stack);
        app.on_import_folder_requested(move || {
            let Some(app) = callback_app.upgrade() else {
                return;
            };
            #[cfg(target_os = "android")]
            {
                android_open_folder();
                app.set_status_message("Select a folder in the Android dialog.".into());
                return;
            }
            #[cfg(not(target_os = "android"))]
            {
                app.set_busy(true);
                app.set_status_message("Selecting and scanning folder…".into());
                let parent_id = folder_stack.borrow().last().copied();
                import_folder_in_background(
                    weak_app.clone(),
                    database_path.clone(),
                    thumbnail_directory.clone(),
                    parent_id,
                );
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        app.on_create_folder_requested(move |name| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let parent_id = folder_stack.borrow().last().copied();
            let database = database.borrow();
            match database
                .create_folder(name.as_str(), parent_id)
                .and_then(|_| refresh_models(&app, &database, parent_id))
            {
                Ok(()) => app.set_status_message("Folder created.".into()),
                Err(error) => {
                    app.set_status_message(format!("Could not create folder: {error}").into())
                }
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        app.on_rename_folder_requested(move |name| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let Some(folder_id) = folder_stack.borrow().last().copied() else {
                return;
            };
            let database = database.borrow();
            match database.rename_folder(folder_id, name.as_str()) {
                Ok(()) => {
                    app.set_folder_title(name.trim().into());
                    let _ = refresh_models(&app, &database, Some(folder_id));
                    app.set_status_message("Folder renamed.".into());
                }
                Err(error) => app.set_status_message(format!("Rename failed: {error}").into()),
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        app.on_search_requested(move |query| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            match database.borrow().search_items(query.as_str()) {
                Ok(items) => app.set_search_items(ui_model(items)),
                Err(error) => app.set_status_message(format!("Search failed: {error}").into()),
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        let selection = Rc::clone(&selection);
        app.on_selection_mode_changed(move |enabled| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            if !enabled {
                selection.borrow_mut().clear();
                app.set_selection_count(0);
            }
            let current_folder = folder_stack.borrow().last().copied();
            let _ = refresh_library_context(
                &app,
                &database.borrow(),
                current_folder,
                &selection.borrow(),
            );
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        let selection = Rc::clone(&selection);
        app.on_selection_toggled(move |entry_id| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let mut selection = selection.borrow_mut();
            if !selection.remove(entry_id.as_str()) {
                selection.insert(entry_id.to_string());
            }
            app.set_selection_count(selection.len() as i32);
            let current_folder = folder_stack.borrow().last().copied();
            let _ = refresh_library_context(&app, &database.borrow(), current_folder, &selection);
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        let selection = Rc::clone(&selection);
        app.on_delete_selected_requested(move || {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let selected = selection.borrow().iter().cloned().collect::<Vec<_>>();
            let current_folder = folder_stack.borrow().last().copied();
            let database = database.borrow();
            let result = (|| -> rusqlite::Result<()> {
                for entry_id in &selected {
                    if let Some(folder_id) = folder_id(entry_id) {
                        database.delete_folder(folder_id)?;
                    } else if let Some(folder_id) = current_folder {
                        database.remove_title_from_folder(folder_id, entry_id)?;
                    } else {
                        database.remove_title_from_library(entry_id)?;
                    }
                }
                refresh_models(&app, &database, current_folder)
            })();

            match result {
                Ok(()) => {
                    selection.borrow_mut().clear();
                    app.set_selection_count(0);
                    app.set_selection_mode(false);
                    app.set_status_message(
                        format!(
                            "Removed {} items from the library; the original files were not deleted.",
                            selected.len()
                        )
                        .into(),
                    );
                }
                Err(error) => app.set_status_message(
                    format!("Could not remove items from the library: {error}").into(),
                ),
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        app.on_open_folder_requested(move |entry_id| {
            let Some(folder_id) = folder_id(entry_id.as_str()) else {
                return;
            };
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let database = database.borrow();
            let result = (|| -> rusqlite::Result<()> {
                let Some(name) = database.folder_name(folder_id)? else {
                    return Ok(());
                };
                app.set_folder_title(name.into());
                app.set_folder_items(ui_model(database.folder_items(folder_id)?));
                app.set_view_mode(1);
                folder_stack.borrow_mut().push(folder_id);
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("Could not open folder: {error}");
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        app.on_folder_back_requested(move || {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            folder_stack.borrow_mut().pop();
            let parent = folder_stack.borrow().last().copied();
            if let Some(parent_id) = parent {
                let database = database.borrow();
                match (
                    database.folder_name(parent_id),
                    database.folder_items(parent_id),
                ) {
                    (Ok(Some(name)), Ok(items)) => {
                        app.set_folder_title(name.into());
                        app.set_folder_items(ui_model(items));
                    }
                    (Err(error), _) | (_, Err(error)) => {
                        eprintln!("Could not return to folder: {error}");
                    }
                    _ => app.set_view_mode(0),
                }
            } else {
                app.set_view_mode(0);
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        app.on_favorite_toggled(move |title_id| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let database = database.borrow();
            let current_folder = folder_stack.borrow().last().copied();
            if let Err(error) = database
                .toggle_favorite(title_id.as_str())
                .and_then(|()| refresh_models(&app, &database, current_folder))
            {
                eprintln!("Could not update favorite: {error}");
            }
        });
    }

    {
        let weak_app = app.as_weak();
        let database = Rc::clone(&database);
        let folder_stack = Rc::clone(&folder_stack);
        app.on_reading_removed(move |title_id| {
            let Some(app) = weak_app.upgrade() else {
                return;
            };
            let database = database.borrow();
            let current_folder = folder_stack.borrow().last().copied();
            if let Err(error) = database
                .remove_from_reading(title_id.as_str())
                .and_then(|()| refresh_models(&app, &database, current_folder))
            {
                eprintln!("Could not remove title from Reading: {error}");
            }
        });
    }

    {
        let worker_app = app.as_weak();
        let worker_database_path = database_path.clone();
        let worker_thumbnail_directory = thumbnail_directory.clone();
        thread::spawn(move || {
            let generated = LibraryDb::open(&worker_database_path)
                .map_err(|error| error.into())
                .and_then(|database| {
                    generate_pending_thumbnails(&database, &worker_thumbnail_directory)
                });
            match generated {
                Ok(0) => {}
                Ok(count) => {
                    finish_background_import(worker_app, format!("Covers prepared: {count}."))
                }
                Err(error) => eprintln!("Could not prepare covers: {error}"),
            }
        });
    }

    #[cfg(target_os = "android")]
    let _android_import_timer = {
        let timer = slint::Timer::default();
        let inbox = data_directory.join("saf-inbox");
        let folder_inbox = data_directory.join("saf-folder-inbox");
        let reference_inbox = data_directory.join("saf-reference-inbox");
        let app_weak = app.as_weak();
        let database_path = database_path.clone();
        let thumbnail_directory = thumbnail_directory.clone();
        let folder_stack = Rc::clone(&folder_stack);
        let processed = Rc::new(RefCell::new(HashSet::<PathBuf>::new()));
        let import_running = Arc::new(AtomicBool::new(false));
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(750),
            move || {
                if import_running.load(Ordering::Acquire) {
                    return;
                }
                let files: Vec<_> = fs::read_dir(&inbox)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.is_file()
                            && path.extension().and_then(|value| value.to_str()) != Some("part")
                            && path.extension().and_then(|value| value.to_str()) != Some("imported")
                            && path.extension().and_then(|value| value.to_str()) != Some("name")
                            && !PathBuf::from(format!("{}.imported", path.display())).is_file()
                            && !processed.borrow().contains(path)
                    })
                    .collect();
                let folder_batches: Vec<_> = fs::read_dir(&folder_inbox)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.is_dir()
                            && path.join(".ready").is_file()
                            && !processed.borrow().contains(path)
                    })
                    .collect();
                let reference_batches: Vec<_> = fs::read_dir(&reference_inbox)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.is_dir()
                            && path.join(".ready").is_file()
                            && !path.join(".imported").is_file()
                            && !processed.borrow().contains(path)
                    })
                    .collect();
                if files.is_empty() && folder_batches.is_empty() && reference_batches.is_empty() {
                    return;
                }
                let Some(app) = app_weak.upgrade() else {
                    return;
                };
                let file_folder_id = folder_stack.borrow().last().copied();
                for path in files
                    .iter()
                    .chain(folder_batches.iter())
                    .chain(reference_batches.iter())
                {
                    processed.borrow_mut().insert(path.clone());
                }
                import_running.store(true, Ordering::Release);
                app.set_busy(true);
                app.set_status_message("Android import and indexing…".into());

                let worker_app = app_weak.clone();
                let worker_database_path = database_path.clone();
                let worker_thumbnail_directory = thumbnail_directory.clone();
                let worker_running = Arc::clone(&import_running);
                thread::spawn(move || {
                    let result = (|| -> Result<usize, Box<dyn Error + Send + Sync>> {
                        let mut imported = 0;
                        let mut database = LibraryDb::open(&worker_database_path)?;
                        for file in files {
                            let name_path = PathBuf::from(format!("{}.name", file.display()));
                            let display_name = fs::read_to_string(&name_path).ok();
                            if database.import_file_named(
                                &file,
                                file_folder_id,
                                display_name.as_deref(),
                            )? {
                                imported += 1;
                            }
                            fs::write(format!("{}.imported", file.display()), [])?;
                        }
                        for batch in folder_batches {
                            if let Ok(roots) = fs::read_dir(&batch) {
                                for root in roots
                                    .flatten()
                                    .map(|entry| entry.path())
                                    .filter(|path| path.is_dir())
                                {
                                    // Android folder import always creates a peer folder in the
                                    // root library. Its files remain inside that imported folder.
                                    let display_name = root
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or("Imported folder");
                                    imported += database
                                        .import_directory_as_new(&root, None, display_name)?
                                        .imported;
                                }
                            }
                            fs::remove_file(batch.join(".ready"))?;
                            fs::write(batch.join(".imported"), [])?;
                        }
                        for batch in reference_batches {
                            imported += import_saf_reference_batch(&mut database, &batch)?;
                            fs::remove_file(batch.join(".ready"))?;
                            fs::write(batch.join(".imported"), [])?;
                        }
                        generate_pending_thumbnails(&database, &worker_thumbnail_directory)?;
                        Ok(imported)
                    })();
                    worker_running.store(false, Ordering::Release);
                    let message = match result {
                        Ok(imported) => format!("Android import complete: {imported} new."),
                        Err(error) => format!("Android import failed: {error}"),
                    };
                    finish_background_import(worker_app, message);
                });
            },
        );
        timer
    };

    app.run()?;
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn main() -> Result<(), Box<dyn Error>> {
    let project_dirs = ProjectDirs::from("rs", "Citac", "Citac")
        .ok_or("Could not determine the application data directory")?;
    run_app(project_dirs.data_dir().to_path_buf())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    let data_directory = app
        .internal_data_path()
        .expect("Android did not provide a private application directory");
    let _ = ANDROID_APP.set(app.clone());
    slint::android::init(app).expect("Could not start the Slint Android backend");
    run_app(data_directory).expect("Could not start Reader");
}

#[cfg(test)]
mod reader_tool_tests {
    use super::*;

    fn page(heading: &str, body: &str, progress: f32) -> DocumentPage {
        DocumentPage {
            heading: heading.to_string(),
            body: body.to_string(),
            locator: format!("test:{progress}"),
            progress,
            blocks: vec![reader::DocumentBlock::Text(body.to_string())],
        }
    }

    #[test]
    fn navigation_maps_source_progress_to_rendered_pages() {
        let middle = page("Poglavlje", "Tekst", 0.5);
        assert_eq!(mapped_reader_index(&middle, 1, 101), 50);
        assert_eq!(mapped_reader_index(&middle, 1, 0), 0);
    }

    #[test]
    fn toc_and_search_skip_empty_or_unrelated_pages() {
        let pages = vec![
            page("", "Naslovna", 0.0),
            page("Prvo poglavlje", "Jedinstvena riječ u tekstu", 0.5),
            page("Drugo poglavlje", "Drugi sadržaj", 1.0),
        ];

        let toc = reader_toc_items(&pages, 9);
        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].page_index, 4);

        let results = reader_search_items(&pages, 9, "JEDINSTVENA");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].page_index, 4);
        assert!(results[0].detail.to_lowercase().contains("jedinstvena"));
    }
}
