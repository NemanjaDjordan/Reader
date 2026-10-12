use std::{
    collections::{HashMap, HashSet},
    env,
    error::Error,
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::Mutex,
};

use slint::{Rgba8Pixel, SharedPixelBuffer};
use zip::ZipArchive;

use crate::reader::{DocumentBlock, DocumentPage, RenderedBlock};

type QtResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct QtEpubSession {
    helper_path: PathBuf,
    _workspace: tempfile::TempDir,
    prepared_root: PathBuf,
    spine_list: PathBuf,
    pages_directory: PathBuf,
    page_count: usize,
    rendered: Mutex<HashSet<usize>>,
    renderer: Mutex<()>,
}

impl QtEpubSession {
    pub fn open(epub_path: &Path) -> QtResult<Self> {
        let helper_path = find_helper()
            .ok_or("Qt EPUB helper renderer is unavailable; using the built-in view")?;
        let cache = tempfile::Builder::new()
            .prefix("citac-qt-epub-")
            .tempdir()?;
        let prepared_root = cache.path().join("book");
        let pages_directory = cache.path().join("pages");
        fs::create_dir_all(&prepared_root)?;
        fs::create_dir_all(&pages_directory)?;
        let spine_list = prepare_epub(epub_path, &prepared_root, cache.path())?;
        let output = run_helper(
            &helper_path,
            &prepared_root,
            &spine_list,
            &pages_directory,
            0,
            0,
        )?;
        let page_count =
            parse_page_count(&output).ok_or("Qt renderer returned an invalid page count")?;
        if page_count == 0 {
            return Err("EPUB has no pages to display".into());
        }
        Ok(Self {
            helper_path,
            _workspace: cache,
            prepared_root,
            spine_list,
            pages_directory,
            page_count,
            rendered: Mutex::new(HashSet::new()),
            renderer: Mutex::new(()),
        })
    }

    pub fn pages(&self) -> Vec<DocumentPage> {
        (0..self.page_count)
            .map(|index| DocumentPage {
                heading: String::new(),
                body: String::new(),
                locator: format!("qt-page:{index}"),
                progress: if self.page_count <= 1 {
                    1.0
                } else {
                    index as f32 / (self.page_count - 1) as f32
                },
                blocks: vec![DocumentBlock::Text(String::new())],
            })
            .collect()
    }

    pub fn render_page(&self, index: usize) -> QtResult<Vec<RenderedBlock>> {
        let path = self.ensure_page(index)?;
        let rgba = image::open(path)?.into_rgba8();
        let (width, height) = rgba.dimensions();
        let pixels =
            SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), width, height);
        Ok(vec![RenderedBlock::Image {
            pixels,
            premultiplied: false,
        }])
    }

    pub fn preload_neighbors(&self, index: usize) {
        if index > 0 {
            let _ = self.ensure_page(index - 1);
        }
        if index + 1 < self.page_count {
            let _ = self.ensure_page(index + 1);
        }
        self.trim_cache(index);
    }

    fn ensure_page(&self, index: usize) -> QtResult<PathBuf> {
        if index >= self.page_count {
            return Err("Requested EPUB page is out of range".into());
        }
        let path = page_path(&self.pages_directory, index);
        if self.rendered.lock().is_ok_and(|set| set.contains(&index)) && path.is_file() {
            return Ok(path);
        }

        let _guard = self
            .renderer
            .lock()
            .map_err(|_| "Qt renderer lock is corrupted")?;
        if self.rendered.lock().is_ok_and(|set| set.contains(&index)) && path.is_file() {
            return Ok(path);
        }
        run_helper(
            &self.helper_path,
            &self.prepared_root,
            &self.spine_list,
            &self.pages_directory,
            1,
            index + 1,
        )?;
        if !path.is_file() {
            return Err(format!("Qt renderer did not create page {}", index + 1).into());
        }
        self.rendered
            .lock()
            .map_err(|_| "Qt cache lock is corrupted")?
            .insert(index);
        Ok(path)
    }

    fn trim_cache(&self, current: usize) {
        let Ok(mut rendered) = self.rendered.lock() else {
            return;
        };
        rendered.retain(|index| {
            let keep = index.abs_diff(current) <= 1;
            if !keep {
                let _ = fs::remove_file(page_path(&self.pages_directory, *index));
            }
            keep
        });
    }
}

fn find_helper() -> Option<PathBuf> {
    if let Some(path) = env::var_os("CITAC_QT_EPUB_HELPER").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    if let Ok(executable) = env::current_exe() {
        let sibling = executable.with_file_name("citac-epub-qt-prototype");
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    let built = PathBuf::from(env!("CITAC_QT_EPUB_HELPER_BUILD"));
    built.is_file().then_some(built)
}

fn run_helper(
    helper: &Path,
    prepared_root: &Path,
    spine_list: &Path,
    output_directory: &Path,
    render_count: usize,
    first_page: usize,
) -> QtResult<String> {
    let output = Command::new(helper)
        .arg(prepared_root)
        .arg(spine_list)
        .arg(output_directory)
        .arg(render_count.to_string())
        .arg("10")
        .arg(first_page.to_string())
        .env("QT_QPA_PLATFORM", "offscreen")
        .output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            format!("Qt renderer exited with status {}", output.status)
        } else {
            message
        }
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_ARCHIVE_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;

fn prepare_epub(epub: &Path, destination: &Path, workspace: &Path) -> QtResult<PathBuf> {
    let mut archive = ZipArchive::new(File::open(epub)?)?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("EPUB contains too many archive entries".into());
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.size() > MAX_ARCHIVE_ENTRY_BYTES {
            return Err(format!("EPUB entry is too large: {}", entry.name()).into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("EPUB size is outside the supported range")?;
        if total > MAX_EXTRACTED_BYTES {
            return Err("EPUB prelazi sigurnosni limit raspakivanja".into());
        }
    }

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("Nebezbjedna EPUB putanja: {}", entry.name()))?
            .to_owned();
        validate_relative(&relative)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("EPUB symbolic links are not allowed: {}", entry.name()).into());
        }
        let target = destination.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(target)?;
        io::copy(&mut entry, &mut output)?;
    }

    let container = fs::read_to_string(destination.join("META-INF/container.xml"))?;
    let container_xml = roxmltree::Document::parse(&container)?;
    let package_relative = container_xml
        .descendants()
        .find(|node| node.has_tag_name("rootfile"))
        .and_then(|node| node.attribute("full-path"))
        .ok_or("EPUB container nema OPF putanju")?;
    let package_relative = PathBuf::from(package_relative);
    validate_relative(&package_relative)?;
    let package = fs::read_to_string(destination.join(&package_relative))?;
    let package_xml = roxmltree::Document::parse(&package)?;
    let manifest = package_xml
        .descendants()
        .filter(|node| node.has_tag_name("item"))
        .filter_map(|node| {
            Some((
                node.attribute("id")?.to_owned(),
                node.attribute("href")?.to_owned(),
            ))
        })
        .collect::<HashMap<_, _>>();
    let package_directory = package_relative.parent().unwrap_or(Path::new(""));
    let mut spine = Vec::new();
    for id in package_xml
        .descendants()
        .filter(|node| node.has_tag_name("itemref"))
        .filter_map(|node| node.attribute("idref"))
    {
        let href = manifest
            .get(id)
            .ok_or_else(|| format!("EPUB spine references an unknown manifest entry: {id}"))?;
        let href = href.split(['#', '?']).next().unwrap_or(href);
        let decoded = percent_encoding::percent_decode_str(href).decode_utf8()?;
        let relative = package_directory.join(decoded.as_ref());
        validate_relative(&relative)?;
        if !destination.join(&relative).is_file() {
            return Err(format!("EPUB spine dokument ne postoji: {}", relative.display()).into());
        }
        spine.push(relative);
    }
    if spine.is_empty() {
        return Err("EPUB spine je prazan".into());
    }
    let spine_list = workspace.join("spine.txt");
    let contents = spine
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&spine_list, contents)?;
    Ok(spine_list)
}

fn validate_relative(path: &Path) -> QtResult<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("Nebezbjedna relativna EPUB putanja: {}", path.display()).into());
    }
    Ok(())
}

fn parse_page_count(output: &str) -> Option<usize> {
    output.lines().find_map(|line| {
        line.strip_prefix("CITAC_RESULT pages=")
            .and_then(|value| value.trim().parse().ok())
    })
}

fn page_path(directory: &Path, index: usize) -> PathBuf {
    directory.join(format!("stranica-{:03}.png", index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_machine_readable_page_count() {
        assert_eq!(
            parse_page_count("tekst\nCITAC_RESULT pages=123\n"),
            Some(123)
        );
    }

    #[test]
    fn rust_preparation_and_qt_render_open_known_fixture() {
        let fixture = Path::new("target/debug/TestFajlovi/Test_sa_vise_slika.epub");
        if !fixture.is_file() {
            return;
        }
        let session = QtEpubSession::open(fixture).unwrap();
        assert_eq!(session.page_count, 123);
        assert!(matches!(
            session.render_page(0).unwrap().as_slice(),
            [RenderedBlock::Image { .. }]
        ));
    }
}
