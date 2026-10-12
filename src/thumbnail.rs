use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use image::{DynamicImage, RgbaImage};

use crate::{library::ThumbnailCandidate, reader};

pub fn generate(
    candidate: &ThumbnailCandidate,
    source: &Path,
    cache_dir: &Path,
) -> Option<PathBuf> {
    if candidate.format == "TXT" {
        return None;
    }
    let document = reader::load_document(source, &candidate.format).ok()?;
    let image = document
        .pages
        .iter()
        .take(8)
        .find_map(|page| rendered_cover(source, &candidate.format, page))?;

    fs::create_dir_all(cache_dir).ok()?;
    let safe_id = candidate
        .id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let output = cache_dir.join(format!("{safe_id}.jpg"));
    let thumbnail = image.thumbnail(200, 300).to_rgb8();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(File::create(&output).ok()?, 72);
    encoder.encode_image(&thumbnail).ok()?;
    Some(output)
}

fn rendered_cover(path: &Path, format: &str, page: &reader::DocumentPage) -> Option<DynamicImage> {
    reader::render_page(path, format, page)
        .ok()?
        .into_iter()
        .find_map(|block| match block {
            reader::RenderedBlock::Image {
                pixels,
                premultiplied,
            } => {
                let width = pixels.width();
                let height = pixels.height();
                let mut bytes = pixels.as_bytes().to_vec();
                if premultiplied {
                    for pixel in bytes.chunks_exact_mut(4) {
                        let alpha = u16::from(pixel[3]);
                        if alpha > 0 {
                            for channel in &mut pixel[..3] {
                                *channel = ((u16::from(*channel) * 255 / alpha).min(255)) as u8;
                            }
                        }
                    }
                }
                RgbaImage::from_raw(width, height, bytes).map(DynamicImage::ImageRgba8)
            }
            reader::RenderedBlock::Text(_) => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    #[test]
    fn cbz_first_page_is_cached_as_bounded_jpeg() {
        let temporary = tempfile::tempdir().unwrap();
        let comic = temporary.path().join("strip.cbz");
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            600,
            900,
            image::Rgba([30, 90, 150, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let file = fs::File::create(&comic).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("001.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(png.get_ref()).unwrap();
        archive.finish().unwrap();

        let candidate = ThumbnailCandidate {
            id: "sha256:test".to_owned(),
            source: comic.to_string_lossy().into_owned(),
            format: "CBZ".to_owned(),
        };
        let output = generate(&candidate, &comic, &temporary.path().join("thumbs")).unwrap();
        let image = image::open(output).unwrap();
        assert!(image.width() <= 200);
        assert!(image.height() <= 300);
        assert_eq!((image.width(), image.height()), (200, 300));
    }
}
