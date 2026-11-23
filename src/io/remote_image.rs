use gtk4::gdk_pixbuf::{PixbufLoader, prelude::*};
use std::time::Duration;

const MAX_DOWNLOAD_BYTES: u64 = 10 * 1024 * 1024;
const MAX_IMAGE_SIDE: i32 = 4096;

pub(crate) fn download_png(url: &str) -> Result<Vec<u8>, &'static str> {
    let uri: ureq::http::Uri = url.parse().map_err(|_| "Enter a valid image URL.")?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.host().is_none() {
        return Err("Enter an HTTP or HTTPS image URL.");
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(5)
        .build()
        .into();
    let mut response = agent
        .get(uri)
        .call()
        .map_err(|_| "Could not download the image.")?;
    if !response.status().is_success() {
        return Err("Could not download the image.");
    }

    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD_BYTES)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => "The image download exceeds 10 MB.",
            _ => "Could not download the image.",
        })?;

    let loader = PixbufLoader::new();
    loader.connect_size_prepared(|loader, width, height| {
        let longest_side = width.max(height);
        if longest_side > MAX_IMAGE_SIDE && width > 0 && height > 0 {
            let scale = f64::from(MAX_IMAGE_SIDE) / f64::from(longest_side);
            loader.set_size(
                (f64::from(width) * scale).round().max(1.0) as i32,
                (f64::from(height) * scale).round().max(1.0) as i32,
            );
        }
    });
    let write = loader.write(&bytes);
    let close = loader.close();
    write
        .and(close)
        .map_err(|_| "This URL did not return a supported image.")?;
    loader
        .pixbuf()
        .ok_or("This URL did not return a supported image.")?
        .save_to_bufferv("png", &[])
        .map_err(|_| "Could not prepare the image for insertion.")
}
