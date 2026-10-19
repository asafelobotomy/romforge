//! Thumbnails for the picture views: made once at the height asked for,
//! reused while the picture is unchanged, made again when it changes. On
//! pictures made in temp folders.

use std::path::Path;
use std::time::{Duration, SystemTime};

use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use romforge_core::thumbs::{self, thumbnail};

fn opaque(path: &Path, w: u32, h: u32, shade: u8) {
    RgbImage::from_pixel(w, h, Rgb([shade, 40, 90]))
        .save(path)
        .unwrap();
}

fn see_through(path: &Path, w: u32, h: u32) {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([200, 30, 30, 255]));
    for y in 0..h {
        img.put_pixel(0, y, Rgba([0, 0, 0, 0]));
    }
    img.save(path).unwrap();
}

fn dims(path: &Path) -> (u32, u32) {
    image::image_dimensions(path).unwrap()
}

fn touch(path: &Path, secs: u64) {
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

#[test]
fn an_opaque_picture_becomes_a_jpeg_at_the_height_asked_for() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, cache) = (tmp.path().join("box.png"), tmp.path().join("cache"));
    opaque(&src, 512, 720, 10);

    let t = thumbnail(&src, 340, &cache).unwrap();
    assert!(t.starts_with(thumbs::dir(&cache)));
    assert_eq!(t.extension().unwrap(), "jpg");
    assert_eq!(image::ImageFormat::from_path(&t).unwrap(), ImageFormat::Jpeg);
    assert_eq!(dims(&t), (242, 340));
}

#[test]
fn a_see_through_picture_stays_a_png() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, cache) = (tmp.path().join("box3d.png"), tmp.path().join("cache"));
    see_through(&src, 800, 1000);

    let t = thumbnail(&src, 340, &cache).unwrap();
    assert_eq!(t.extension().unwrap(), "png");
    let img = image::open(&t).unwrap();
    assert_eq!(img.height(), 340);
    assert!(matches!(img, DynamicImage::ImageRgba8(_)));
    assert!(img.to_rgba8().get_pixel(0, 170).0[3] < 255);
}

#[test]
fn a_thumbnail_is_made_once_and_reused() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, cache) = (tmp.path().join("hero.jpg"), tmp.path().join("cache"));
    opaque(&src, 1920, 1080, 10);
    touch(&src, 1_000_000);

    let first = thumbnail(&src, 480, &cache).unwrap();
    let made = std::fs::metadata(&first).unwrap().modified().unwrap();
    // The same file comes back, not written again.
    std::thread::sleep(Duration::from_millis(20));
    let again = thumbnail(&src, 480, &cache).unwrap();
    assert_eq!(first, again);
    assert_eq!(std::fs::metadata(&again).unwrap().modified().unwrap(), made);
    // Another height is another thumbnail.
    let other = thumbnail(&src, 240, &cache).unwrap();
    assert_ne!(other, first);
    assert_eq!(dims(&other), (427, 240));
    assert_eq!(std::fs::read_dir(thumbs::dir(&cache)).unwrap().count(), 2);
}

#[test]
fn a_changed_picture_gets_a_new_thumbnail() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, cache) = (tmp.path().join("box.png"), tmp.path().join("cache"));
    opaque(&src, 512, 720, 10);
    touch(&src, 1_000_000);
    let old = thumbnail(&src, 340, &cache).unwrap();

    // A new scan of another size: a new thumbnail, of the new picture.
    opaque(&src, 600, 680, 250);
    touch(&src, 2_000_000);
    let new = thumbnail(&src, 340, &cache).unwrap();
    assert_ne!(old, new);
    assert_eq!(dims(&new), (300, 340));
    let red = image::open(&new).unwrap().to_rgb8().get_pixel(150, 170).0[0];
    assert!(red > 200, "shows the new picture, red {red}");

    // Same size, only the time changed: still made again.
    touch(&src, 3_000_000);
    assert_ne!(thumbnail(&src, 340, &cache).unwrap(), new);
}

#[test]
fn a_small_picture_is_served_as_it_is() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, cache) = (tmp.path().join("small.png"), tmp.path().join("cache"));
    opaque(&src, 100, 140, 10);
    assert_eq!(thumbnail(&src, 340, &cache).unwrap(), src);
    assert_eq!(thumbnail(&src, 340, &cache).unwrap(), src);
    // A height no card asks for: the original too.
    let big = tmp.path().join("big.png");
    opaque(&big, 100, 5000, 10);
    assert_eq!(thumbnail(&big, 4000, &cache).unwrap(), big);
}

#[test]
fn a_broken_picture_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, cache) = (tmp.path().join("box.png"), tmp.path().join("cache"));
    std::fs::write(&src, b"not a picture").unwrap();
    assert!(thumbnail(&src, 340, &cache).is_err());
    assert!(thumbnail(&tmp.path().join("gone.png"), 340, &cache).is_err());
}
