use std::{
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

use encoding_rs::WINDOWS_1252;
use zip::ZipArchive;

pub type ReaderResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const EPUB_PAGE_TARGET_CHARS: usize = 2_450;

pub struct LoadedDocument {
    pub pages: Vec<DocumentPage>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentPage {
    pub heading: String,
    pub body: String,
    pub locator: String,
    pub progress: f32,
    pub blocks: Vec<DocumentBlock>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DocumentBlock {
    Text(String),
    ArchiveImage(String),
    InlineSvg(String),
}

#[derive(Clone)]
pub enum RenderedBlock {
    Text(String),
    Image {
        pixels: slint::SharedPixelBuffer<slint::Rgba8Pixel>,
        premultiplied: bool,
    },
}

#[cfg(target_os = "android")]
const COMIC_CACHE_LIMIT_BYTES: usize = 48 * 1024 * 1024;
#[cfg(not(target_os = "android"))]
const COMIC_CACHE_LIMIT_BYTES: usize = 96 * 1024 * 1024;

struct CachedComicPage {
    blocks: Vec<RenderedBlock>,
    bytes: usize,
}

pub struct ComicRenderCache {
    path: PathBuf,
    format: String,
    pages: Vec<DocumentPage>,
    entries: Mutex<HashMap<usize, CachedComicPage>>,
}

impl ComicRenderCache {
    pub fn new(path: PathBuf, format: String, pages: Vec<DocumentPage>) -> Self {
        Self {
            path,
            format,
            pages,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn render_page(&self, index: usize) -> ReaderResult<Vec<RenderedBlock>> {
        if let Some(blocks) = self
            .entries
            .lock()
            .map_err(|_| "Comic cache is corrupted")?
            .get(&index)
            .map(|entry| entry.blocks.clone())
        {
            return Ok(blocks);
        }
        let page = self.pages.get(index).ok_or("Comic page is out of range")?;
        let blocks = render_page(&self.path, &self.format, page)?;
        let bytes = rendered_bytes(&blocks);
        if bytes <= COMIC_CACHE_LIMIT_BYTES {
            self.entries
                .lock()
                .map_err(|_| "Comic cache is corrupted")?
                .insert(
                    index,
                    CachedComicPage {
                        blocks: blocks.clone(),
                        bytes,
                    },
                );
        }
        Ok(blocks)
    }

    pub fn preload_neighbors(&self, current: usize) {
        if current > 0 {
            let _ = self.render_page(current - 1);
        }
        if current + 1 < self.pages.len() {
            let _ = self.render_page(current + 1);
        }
        self.trim(current);
    }

    fn trim(&self, current: usize) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        entries.retain(|index, _| index.abs_diff(current) <= 1);
        let mut total = entries.values().map(|entry| entry.bytes).sum::<usize>();
        for index in [current.saturating_sub(1), current.saturating_add(1)] {
            if total <= COMIC_CACHE_LIMIT_BYTES {
                break;
            }
            if index != current
                && let Some(removed) = entries.remove(&index)
            {
                total = total.saturating_sub(removed.bytes);
            }
        }
    }
}

fn rendered_bytes(blocks: &[RenderedBlock]) -> usize {
    blocks
        .iter()
        .map(|block| match block {
            RenderedBlock::Text(text) => text.len(),
            RenderedBlock::Image { pixels, .. } => {
                pixels.width() as usize * pixels.height() as usize * 4
            }
        })
        .sum()
}

pub fn load_document(path: &Path, format: &str) -> ReaderResult<LoadedDocument> {
    match format {
        "TXT" => Ok(LoadedDocument {
            pages: load_txt(path)?,
        }),
        "EPUB" => load_epub(path),
        "CBZ" => load_cbz(path),
        "CBR" => load_cbr(path),
        _ => Err(format!("Format {format} does not have a text reader yet").into()),
    }
}

fn load_cbr(path: &Path) -> ReaderResult<LoadedDocument> {
    let archive = unrar::Archive::new(path).open_for_listing()?;
    let mut images = Vec::new();
    let mut total_size = 0_u64;
    let mut entry_count = 0_usize;
    for entry in archive {
        let entry = entry?;
        entry_count += 1;
        if entry_count > 10_000 {
            return Err("CBR contains too many archive entries".into());
        }
        let relative = normalized_rar_path(&entry.filename)?;
        total_size = total_size
            .checked_add(entry.unpacked_size)
            .ok_or("CBR size is outside the supported range")?;
        if total_size > 1024 * 1024 * 1024 {
            return Err("CBR exceeds the safe unpacked-size limit".into());
        }
        if !entry.is_directory() && is_comic_image(&relative) {
            if entry.unpacked_size > 32 * 1024 * 1024 {
                return Err(format!("CBR image is too large: {relative}").into());
            }
            images.push(relative);
        }
    }
    comic_pages(images, "CBR")
}

fn load_cbz(path: &Path) -> ReaderResult<LoadedDocument> {
    let mut archive = ZipArchive::new(fs::File::open(path)?)?;
    if archive.len() > 10_000 {
        return Err("CBZ contains too many archive entries".into());
    }
    let mut total_size = 0_u64;
    let mut images = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.enclosed_name().is_none()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("CBZ contains an unsafe path: {}", entry.name()).into());
        }
        total_size = total_size
            .checked_add(entry.size())
            .ok_or("CBZ size is outside the supported range")?;
        if total_size > 1024 * 1024 * 1024 {
            return Err("CBZ exceeds the safe unpacked-size limit".into());
        }
        if !entry.is_dir() && is_comic_image(entry.name()) {
            if entry.size() > 32 * 1024 * 1024 {
                return Err(format!("CBZ image is too large: {}", entry.name()).into());
            }
            images.push(entry.name().to_owned());
        }
    }
    comic_pages(images, "CBZ")
}

fn comic_pages(mut images: Vec<String>, format: &str) -> ReaderResult<LoadedDocument> {
    images.sort_by(|left, right| natural_cmp(left, right));
    if images.is_empty() {
        return Err(format!("{format} does not contain supported images").into());
    }
    let count = images.len();
    let pages = images
        .into_iter()
        .enumerate()
        .map(|(index, archive_path)| DocumentPage {
            heading: String::new(),
            body: String::new(),
            locator: format!("image:{archive_path}"),
            progress: if count <= 1 {
                1.0
            } else {
                index as f32 / (count - 1) as f32
            },
            blocks: vec![DocumentBlock::ArchiveImage(archive_path)],
        })
        .collect();
    Ok(LoadedDocument { pages })
}

fn normalized_rar_path(path: &Path) -> ReaderResult<String> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("Nebezbjedna CBR putanja: {normalized}").into());
    }
    Ok(normalized)
}

fn is_comic_image(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp")
    )
}

fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    let mut left_chars = left.chars().peekable();
    let mut right_chars = right.chars().peekable();
    loop {
        match (left_chars.peek(), right_chars.peek()) {
            (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit() => {
                let left_number: String =
                    std::iter::from_fn(|| left_chars.next_if(|c| c.is_ascii_digit())).collect();
                let right_number: String =
                    std::iter::from_fn(|| right_chars.next_if(|c| c.is_ascii_digit())).collect();
                let left_trimmed = left_number.trim_start_matches('0');
                let right_trimmed = right_number.trim_start_matches('0');
                let length_order = left_trimmed.len().cmp(&right_trimmed.len());
                if length_order != Ordering::Equal {
                    return length_order;
                }
                let value_order = left_trimmed.cmp(right_trimmed);
                if value_order != Ordering::Equal {
                    return value_order;
                }
                let zeros_order = left_number.len().cmp(&right_number.len());
                if zeros_order != Ordering::Equal {
                    return zeros_order;
                }
            }
            (Some(_), Some(_)) => {
                let order = left_chars.next().cmp(&right_chars.next());
                if order != Ordering::Equal {
                    return order;
                }
            }
            (None, None) => return left.cmp(&right),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
}

fn load_txt(path: &Path) -> ReaderResult<Vec<DocumentPage>> {
    let bytes = fs::read(path)?;
    let text = decode_text(&bytes);
    Ok(chunk_text(&text, 2_400))
}

fn decode_text(bytes: &[u8]) -> String {
    if let Some(content) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(content).into_owned();
    }
    if let Some(content) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(content, true);
    }
    if let Some(content) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(content, false);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    let (text, _, _) = WINDOWS_1252.decode(bytes);
    text.into_owned()
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    char::decode_utf16(units)
        .map(|character| character.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn chunk_text(text: &str, target_chars: usize) -> Vec<DocumentPage> {
    if text.is_empty() {
        return vec![DocumentPage {
            heading: String::new(),
            body: String::new(),
            locator: "char:0".to_owned(),
            progress: 0.0,
            blocks: vec![DocumentBlock::Text(String::new())],
        }];
    }

    let mut pages = Vec::new();
    let mut start_byte = 0;
    let mut start_char = 0;
    while start_byte < text.len() {
        let remainder = &text[start_byte..];
        let mut end_relative = remainder.len();
        let mut consumed_chars = remainder.chars().count();
        if consumed_chars > target_chars {
            let target_byte = remainder
                .char_indices()
                .nth(target_chars)
                .map(|(index, _)| index)
                .unwrap_or(remainder.len());
            let search = &remainder[..target_byte];
            end_relative = search
                .rfind("\n\n")
                .filter(|index| *index > target_byte / 2)
                .map(|index| index + 2)
                .unwrap_or(target_byte);
            consumed_chars = remainder[..end_relative].chars().count();
        }
        let body = remainder[..end_relative].trim().to_owned();
        pages.push(DocumentPage {
            heading: String::new(),
            blocks: vec![DocumentBlock::Text(body.clone())],
            body,
            locator: format!("char:{start_char}"),
            progress: start_char as f32 / text.chars().count() as f32,
        });
        start_byte += end_relative;
        start_char += consumed_chars;
    }
    pages
}

fn load_epub(path: &Path) -> ReaderResult<LoadedDocument> {
    let file = fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let container = read_archive_text(&mut archive, "META-INF/container.xml")?;
    let container_xml = roxmltree::Document::parse(&container)?;
    let package_path = container_xml
        .descendants()
        .find(|node| node.has_tag_name("rootfile"))
        .and_then(|node| node.attribute("full-path"))
        .ok_or("EPUB container nema rootfile putanju")?
        .to_owned();
    let package = read_archive_text(&mut archive, &package_path)?;
    let package_xml = roxmltree::Document::parse(&package)?;
    let base = Path::new(&package_path).parent().unwrap_or(Path::new(""));

    let manifest = package_xml
        .descendants()
        .filter(|node| node.has_tag_name("item"))
        .filter_map(|node| Some((node.attribute("id")?, node.attribute("href")?)))
        .collect::<HashMap<_, _>>();
    let spine = package_xml
        .descendants()
        .filter(|node| node.has_tag_name("itemref"))
        .filter_map(|node| node.attribute("idref"))
        .collect::<Vec<_>>();

    let declared_original = package_xml.descendants().any(|node| {
        if !node.has_tag_name("meta") {
            return false;
        }
        let property_match = node.attribute("property") == Some("rendition:layout")
            && node
                .text()
                .is_some_and(|text| text.trim() == "pre-paginated");
        let legacy_match = node.attribute("name") == Some("rendition:layout")
            && node.attribute("content") == Some("pre-paginated");
        property_match || legacy_match
    });

    let mut chapters = Vec::new();
    for (index, idref) in spine.iter().enumerate() {
        let Some(href) = manifest.get(idref) else {
            continue;
        };
        let archive_path = normalized_archive_path(base.join(decoded_href(href)));
        let xhtml = read_archive_text(&mut archive, &archive_path)?;
        let document = roxmltree::Document::parse(&xhtml)?;
        let body = document
            .descendants()
            .find(|node| node.has_tag_name("body"))
            .unwrap_or_else(|| document.root_element());
        let heading = body
            .descendants()
            .find(|node| matches!(node.tag_name().name(), "h1" | "h2" | "title"))
            .and_then(|node| node.text())
            .unwrap_or("")
            .trim()
            .to_owned();
        let chapter_base = Path::new(&archive_path).parent().unwrap_or(Path::new(""));
        let blocks = extract_epub_blocks(body, &xhtml, chapter_base);
        if !blocks.is_empty() {
            chapters.push((index, heading, blocks));
        }
    }

    let image_page_layout = !chapters.is_empty()
        && chapters.iter().all(|(_, _, blocks)| {
            let image_count = blocks
                .iter()
                .filter(|block| {
                    matches!(
                        block,
                        DocumentBlock::ArchiveImage(_) | DocumentBlock::InlineSvg(_)
                    )
                })
                .count();
            let has_text = blocks
                .iter()
                .any(|block| matches!(block, DocumentBlock::Text(text) if !text.trim().is_empty()));
            image_count == 1 && !has_text
        });
    let preserve_spine_pages = declared_original || image_page_layout;

    let mut pages = Vec::new();
    for (index, heading, blocks) in chapters {
        if preserve_spine_pages {
            push_epub_page(&mut pages, index, &heading, blocks);
        } else {
            pages.extend(paginate_epub_blocks(index, heading, blocks));
        }
    }
    if pages.is_empty() {
        return Err("EPUB has no readable spine chapters".into());
    }
    let page_count = pages.len();
    for (index, page) in pages.iter_mut().enumerate() {
        page.progress = index as f32 / page_count.max(1) as f32;
    }
    Ok(LoadedDocument { pages })
}

fn paginate_epub_blocks(
    spine_index: usize,
    heading: String,
    blocks: Vec<DocumentBlock>,
) -> Vec<DocumentPage> {
    let mut pages = Vec::new();
    let mut current = Vec::new();
    let mut current_chars = 0;

    for block in blocks {
        match block {
            DocumentBlock::Text(text) => {
                let mut remainder = text.as_str();
                while !remainder.is_empty() {
                    if current_chars >= EPUB_PAGE_TARGET_CHARS {
                        push_epub_page(
                            &mut pages,
                            spine_index,
                            &heading,
                            std::mem::take(&mut current),
                        );
                        current_chars = 0;
                    }

                    let available = EPUB_PAGE_TARGET_CHARS - current_chars;
                    let (piece, rest) = split_text_at(remainder, available);
                    if !piece.is_empty() {
                        current_chars += piece.chars().count();
                        current.push(DocumentBlock::Text(piece.to_owned()));
                    }
                    remainder = rest;

                    if !remainder.is_empty() {
                        push_epub_page(
                            &mut pages,
                            spine_index,
                            &heading,
                            std::mem::take(&mut current),
                        );
                        current_chars = 0;
                    }
                }
            }
            image => current.push(image),
        }
    }

    if !current.is_empty() {
        push_epub_page(&mut pages, spine_index, &heading, current);
    }
    pages
}

fn split_text_at(text: &str, max_chars: usize) -> (&str, &str) {
    if max_chars == 0 {
        return ("", text);
    }
    let Some((limit, _)) = text.char_indices().nth(max_chars) else {
        return (text, "");
    };
    let candidate = &text[..limit];
    let split = candidate
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())
        .map(|(index, character)| index + character.len_utf8())
        .filter(|index| *index >= limit / 2)
        .unwrap_or(limit);
    (text[..split].trim_end(), text[split..].trim_start())
}

fn push_epub_page(
    pages: &mut Vec<DocumentPage>,
    spine_index: usize,
    heading: &str,
    blocks: Vec<DocumentBlock>,
) {
    if blocks.is_empty() {
        return;
    }
    let part = pages
        .iter()
        .rev()
        .take_while(|page| page.locator.starts_with(&format!("spine:{spine_index}:")))
        .count();
    let body = blocks
        .iter()
        .filter_map(|block| match block {
            DocumentBlock::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    pages.push(DocumentPage {
        heading: if part == 0 {
            heading.to_owned()
        } else {
            String::new()
        },
        body,
        locator: format!("spine:{spine_index}:page:{part}"),
        progress: 0.0,
        blocks,
    });
}

pub fn render_page(
    path: &Path,
    format: &str,
    page: &DocumentPage,
) -> ReaderResult<Vec<RenderedBlock>> {
    if format == "TXT" {
        return Ok(page
            .blocks
            .iter()
            .filter_map(|block| match block {
                DocumentBlock::Text(text) => Some(RenderedBlock::Text(text.clone())),
                _ => None,
            })
            .collect());
    }

    if format == "CBR" {
        let archive_path = page
            .blocks
            .iter()
            .find_map(|block| match block {
                DocumentBlock::ArchiveImage(path) => Some(path.as_str()),
                _ => None,
            })
            .ok_or("CBR page does not contain an image")?;
        return render_cbr_image(path, archive_path);
    }

    let file = fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut rendered = Vec::new();
    for block in &page.blocks {
        match block {
            DocumentBlock::Text(text) => rendered.push(RenderedBlock::Text(text.clone())),
            DocumentBlock::ArchiveImage(archive_path) => {
                let result = read_archive_bytes(&mut archive, archive_path).and_then(|bytes| {
                    if archive_path.to_ascii_lowercase().ends_with(".svg") {
                        render_svg(&bytes)
                    } else {
                        decode_raster(&bytes)
                    }
                });
                match result {
                    Ok(image) => rendered.push(image),
                    Err(_) => rendered.push(RenderedBlock::Text(
                        "[Image could not be loaded]".to_owned(),
                    )),
                }
            }
            DocumentBlock::InlineSvg(svg) => match render_svg(svg.as_bytes()) {
                Ok(image) => rendered.push(image),
                Err(_) => rendered.push(RenderedBlock::Text(
                    "[SVG could not be displayed]".to_owned(),
                )),
            },
        }
    }
    Ok(rendered)
}

fn render_cbr_image(path: &Path, wanted: &str) -> ReaderResult<Vec<RenderedBlock>> {
    let mut archive = unrar::Archive::new(path).open_for_processing()?;
    loop {
        let Some(entry_archive) = archive.read_header()? else {
            return Err(format!("CBR image was not found: {wanted}").into());
        };
        let entry_path = normalized_rar_path(&entry_archive.entry().filename)?;
        if entry_path == wanted {
            if entry_archive.entry().unpacked_size > 32 * 1024 * 1024 {
                return Err("CBR image exceeds the allowed size limit".into());
            }
            let (bytes, _) = entry_archive.read()?;
            return Ok(vec![decode_raster(&bytes)?]);
        }
        archive = entry_archive.skip()?;
    }
}

fn extract_epub_blocks(
    body: roxmltree::Node<'_, '_>,
    source: &str,
    chapter_base: &Path,
) -> Vec<DocumentBlock> {
    let mut blocks = Vec::new();
    let mut pending = String::new();
    walk_epub_node(body, source, chapter_base, &mut pending, &mut blocks);
    flush_text(&mut pending, &mut blocks);
    blocks
}

fn walk_epub_node(
    node: roxmltree::Node<'_, '_>,
    source: &str,
    chapter_base: &Path,
    pending: &mut String,
    blocks: &mut Vec<DocumentBlock>,
) {
    if node.is_text() {
        if let Some(text) = node.text() {
            let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !normalized.is_empty() {
                if !pending.is_empty() && !pending.ends_with([' ', '\n']) {
                    pending.push(' ');
                }
                pending.push_str(&normalized);
            }
        }
        return;
    }
    if !node.is_element() {
        return;
    }

    let tag = node.tag_name().name().to_ascii_lowercase();
    if matches!(tag.as_str(), "script" | "style" | "head") {
        return;
    }
    if tag == "img" {
        flush_text(pending, blocks);
        if let Some(src) = node.attribute("src") {
            blocks.push(DocumentBlock::ArchiveImage(normalized_archive_path(
                chapter_base.join(decoded_href(src)),
            )));
        }
        return;
    }
    if tag == "svg" {
        flush_text(pending, blocks);
        let referenced_images = node
            .descendants()
            .filter(|descendant| descendant.has_tag_name("image"))
            .filter_map(|image| {
                image
                    .attributes()
                    .find(|attribute| attribute.name() == "href")
                    .map(|attribute| attribute.value())
            })
            .collect::<Vec<_>>();
        let has_vector_content = node.descendants().any(|descendant| {
            descendant.is_element()
                && matches!(
                    descendant.tag_name().name(),
                    "path"
                        | "rect"
                        | "circle"
                        | "ellipse"
                        | "line"
                        | "polyline"
                        | "polygon"
                        | "text"
                        | "use"
                )
        });
        if referenced_images.len() == 1 && !has_vector_content {
            blocks.push(DocumentBlock::ArchiveImage(normalized_archive_path(
                chapter_base.join(decoded_href(referenced_images[0])),
            )));
        } else {
            blocks.push(DocumentBlock::InlineSvg(source[node.range()].to_owned()));
        }
        return;
    }

    let block_element = matches!(
        tag.as_str(),
        "p" | "div"
            | "section"
            | "article"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "blockquote"
            | "pre"
    );
    if block_element {
        flush_text(pending, blocks);
    }
    if tag == "br" {
        pending.push('\n');
    }
    for child in node.children() {
        walk_epub_node(child, source, chapter_base, pending, blocks);
    }
    if block_element {
        flush_text(pending, blocks);
    }
}

fn flush_text(pending: &mut String, blocks: &mut Vec<DocumentBlock>) {
    let text = pending.trim();
    if !text.is_empty() {
        blocks.push(DocumentBlock::Text(text.to_owned()));
    }
    pending.clear();
}

fn read_archive_bytes<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> ReaderResult<Vec<u8>> {
    let mut entry = archive.by_name(name)?;
    if entry.size() > 32 * 1024 * 1024 {
        return Err("EPUB image exceeds the allowed size limit".into());
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn decode_raster(bytes: &[u8]) -> ReaderResult<RenderedBlock> {
    let mut image_reader =
        image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    image_reader.limits(limits);
    let image = image_reader.decode()?.into_rgba8();
    let (width, height) = image.dimensions();
    let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        image.as_raw(),
        width,
        height,
    );
    Ok(RenderedBlock::Image {
        pixels,
        premultiplied: false,
    })
}

fn render_svg(bytes: &[u8]) -> ReaderResult<RenderedBlock> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &options)?;
    let original = tree.size();
    let scale = (2048.0 / original.width())
        .min(2048.0 / original.height())
        .min(1.0);
    let width = (original.width() * scale).round().max(1.0) as u32;
    let height = (original.height() * scale).round().max(1.0) as u32;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or("SVG dimensions are invalid")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        pixmap.data(),
        width,
        height,
    );
    Ok(RenderedBlock::Image {
        pixels,
        premultiplied: true,
    })
}

fn read_archive_text<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> ReaderResult<String> {
    let mut entry = archive.by_name(name)?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(decode_text(&bytes))
}

fn normalized_archive_path(path: PathBuf) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    parts.join("/")
}

fn decoded_href(href: &str) -> String {
    let path = href.split(['#', '?']).next().unwrap_or(href);
    percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    #[test]
    fn txt_supports_utf8_and_utf16() {
        assert_eq!(decode_text("Ćirilica".as_bytes()), "Ćirilica");
        let utf16 = [0xFF, 0xFE, b'A', 0, 0x07, 0x01];
        assert_eq!(decode_text(&utf16), "Ać");
    }

    #[test]
    fn txt_chunks_keep_precise_character_locators() {
        let pages = chunk_text("abcdef", 3);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].locator, "char:0");
        assert_eq!(pages[1].locator, "char:3");
        assert_eq!(pages[0].body, "abc");
        assert_eq!(pages[1].body, "def");
    }

    #[test]
    fn cbz_pages_use_natural_image_order() {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(temporary.reopen().unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for name in [
            "strana-10.jpg",
            "strana-2.png",
            "notes.txt",
            "strana-1.webp",
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();

        let document = load_cbz(temporary.path()).unwrap();
        let paths = document
            .pages
            .iter()
            .map(|page| match &page.blocks[0] {
                DocumentBlock::ArchiveImage(path) => path.as_str(),
                _ => panic!("očekivana CBZ slika"),
            })
            .collect::<Vec<_>>();
        assert_eq!(paths, ["strana-1.webp", "strana-2.png", "strana-10.jpg"]);
        assert_eq!(document.pages[0].locator, "image:strana-1.webp");
        assert_eq!(document.pages[2].progress, 1.0);
    }

    #[test]
    fn cbz_renders_real_png_page() {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(temporary.reopen().unwrap());
        let options = zip::write::SimpleFileOptions::default();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            3,
            2,
            image::Rgba([20, 40, 60, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        zip.start_file("001.png", options).unwrap();
        zip.write_all(png.get_ref()).unwrap();
        zip.finish().unwrap();

        let document = load_cbz(temporary.path()).unwrap();
        let rendered = render_page(temporary.path(), "CBZ", &document.pages[0]).unwrap();
        match &rendered[0] {
            RenderedBlock::Image { pixels, .. } => {
                assert_eq!((pixels.width(), pixels.height()), (3, 2));
            }
            _ => panic!("očekivana dekodirana CBZ slika"),
        }
    }

    #[test]
    fn cbz_rejects_corrupt_archive_and_unsafe_path() {
        let corrupt = tempfile::NamedTempFile::new().unwrap();
        fs::write(corrupt.path(), b"nije zip arhiva").unwrap();
        assert!(load_cbz(corrupt.path()).is_err());

        let unsafe_archive = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(unsafe_archive.reopen().unwrap());
        zip.start_file("../stranica.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
        assert!(load_cbz(unsafe_archive.path()).is_err());
    }

    #[test]
    fn cbr_fixture_lists_and_renders_first_page_when_available() {
        let fixture = Path::new("target/debug/TestFajlovi/TEST.cbr");
        if !fixture.is_file() {
            return;
        }
        let document = load_cbr(fixture).unwrap();
        assert!(!document.pages.is_empty());
        assert!(matches!(
            render_page(fixture, "CBR", &document.pages[0])
                .unwrap()
                .as_slice(),
            [RenderedBlock::Image { .. }]
        ));

        let cache =
            ComicRenderCache::new(fixture.to_owned(), "CBR".to_owned(), document.pages.clone());
        cache.render_page(0).unwrap();
        cache.preload_neighbors(0);
        let entries = cache.entries.lock().unwrap();
        assert!(entries.len() <= 2);
        assert!(entries.keys().all(|index| *index <= 1));
        assert!(
            entries.values().map(|entry| entry.bytes).sum::<usize>() <= COMIC_CACHE_LIMIT_BYTES
        );
    }

    #[test]
    fn epub_reads_spine_in_declared_order() {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(temporary.reopen().unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(br#"<container><rootfiles><rootfile full-path="OPS/book.opf"/></rootfiles></container>"#).unwrap();
        zip.start_file("OPS/book.opf", options).unwrap();
        zip.write_all(br#"<package><manifest><item id="b" href="b.xhtml"/><item id="a" href="a.xhtml"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#).unwrap();
        zip.start_file("OPS/a.xhtml", options).unwrap();
        zip.write_all(
            b"<html><body><h1>Prvo</h1><p>A</p><img src=\"pic%20one.png\"/><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\" fill=\"red\"/></svg><p>Kraj</p></body></html>",
        )
        .unwrap();
        zip.start_file("OPS/b.xhtml", options).unwrap();
        zip.write_all(b"<html><body><h1>Drugo</h1><p>B</p></body></html>")
            .unwrap();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            1,
            image::Rgba([10, 20, 30, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        zip.start_file("OPS/pic one.png", options).unwrap();
        zip.write_all(png.get_ref()).unwrap();
        zip.finish().unwrap();

        let document = load_epub(temporary.path()).unwrap();
        let pages = document.pages;
        assert_eq!(pages[0].heading, "Prvo");
        assert_eq!(pages[1].heading, "Drugo");
        assert_eq!(pages[0].locator, "spine:0:page:0");
        assert_eq!(pages[0].body, "Prvo\n\nA\n\nKraj");
        assert!(matches!(&pages[0].blocks[0], DocumentBlock::Text(text) if text == "Prvo"));
        assert!(matches!(&pages[0].blocks[1], DocumentBlock::Text(text) if text == "A"));
        assert!(
            matches!(&pages[0].blocks[2], DocumentBlock::ArchiveImage(path) if path == "OPS/pic one.png")
        );
        assert!(matches!(&pages[0].blocks[3], DocumentBlock::InlineSvg(_)));
        assert!(matches!(&pages[0].blocks[4], DocumentBlock::Text(text) if text == "Kraj"));

        let rendered = render_page(temporary.path(), "EPUB", &pages[0]).unwrap();
        assert_eq!(
            rendered
                .iter()
                .filter(|block| matches!(block, RenderedBlock::Image { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn epub_long_spine_is_paginated() {
        let text = "riječ ".repeat(1_000);
        let pages =
            paginate_epub_blocks(3, "Poglavlje".to_owned(), vec![DocumentBlock::Text(text)]);

        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].heading, "Poglavlje");
        assert!(pages[1].heading.is_empty());
        assert_eq!(pages[2].locator, "spine:3:page:2");
        assert!(pages.iter().all(|page| page.body.chars().count() <= 2_450));
    }

    #[test]
    fn epub_svg_image_wrapper_uses_archived_cover() {
        let source = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="../cover.jpeg"/></svg></body></html>"#;
        let document = roxmltree::Document::parse(source).unwrap();
        let body = document
            .descendants()
            .find(|node| node.has_tag_name("body"))
            .unwrap();
        let blocks = extract_epub_blocks(body, source, Path::new("OPS/Text"));

        assert_eq!(
            blocks,
            vec![DocumentBlock::ArchiveImage("OPS/cover.jpeg".to_owned())]
        );
    }
}
