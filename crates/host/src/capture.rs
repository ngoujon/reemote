use anyhow::{Context, Result};
use image::RgbaImage;
use reemote_protocol::{DisplayInfo, FrameChunk};
use xcap::Monitor;

pub fn list_displays() -> Result<Vec<DisplayInfo>> {
    let monitors = Monitor::all().context("failed to enumerate monitors")?;
    monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            Ok(DisplayInfo {
                id: i as u32,
                name: m.name().unwrap_or_default(),
                width: m.width().context("monitor width unavailable")?,
                height: m.height().context("monitor height unavailable")?,
                is_primary: m.is_primary().unwrap_or(false),
            })
        })
        .collect()
}

/// Captures frames from one monitor and produces JPEG-encoded chunks
/// covering only the rows that changed since the previous capture, to keep
/// bandwidth down for mostly-static desktops.
pub struct CaptureSession {
    monitor: Monitor,
    display_id: u32,
    prev: Option<RgbaImage>,
}

impl CaptureSession {
    pub fn new(display_id: u32) -> Result<Self> {
        let monitors = Monitor::all().context("failed to enumerate monitors")?;
        let monitor = monitors
            .into_iter()
            .nth(display_id as usize)
            .context("selected display not found")?;
        Ok(Self {
            monitor,
            display_id,
            prev: None,
        })
    }

    /// Returns `Ok(None)` when nothing changed since the last capture.
    pub fn capture_chunk(&mut self) -> Result<Option<FrameChunk>> {
        let img = self.monitor.capture_image().context("screen capture failed")?;
        let (w, h) = (img.width(), img.height());

        let region = match &self.prev {
            Some(prev) if prev.width() == w && prev.height() == h => diff_rows(prev, &img),
            _ => Some((0, 0, w, h)),
        };

        let Some((x, y, rw, rh)) = region else {
            self.prev = Some(img);
            return Ok(None);
        };

        let sub_rgba = image::imageops::crop_imm(&img, x, y, rw, rh).to_image();
        let sub_rgb = image::DynamicImage::ImageRgba8(sub_rgba).to_rgb8();

        let mut jpeg = Vec::new();
        {
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 60);
            encoder.encode_image(&sub_rgb)?;
        }

        self.prev = Some(img);

        Ok(Some(FrameChunk {
            display_id: self.display_id,
            full_width: w,
            full_height: h,
            x,
            y,
            width: rw,
            height: rh,
            jpeg,
        }))
    }
}

/// Finds the smallest full-width horizontal strip that contains every
/// changed pixel between two same-sized frames, by comparing raw row bytes.
fn diff_rows(prev: &RgbaImage, curr: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let width = curr.width();
    let height = curr.height() as usize;
    let stride = (width * 4) as usize;
    let prev_buf = prev.as_raw();
    let curr_buf = curr.as_raw();

    let mut top = None;
    let mut bottom = None;
    for row in 0..height {
        let start = row * stride;
        let end = start + stride;
        if prev_buf[start..end] != curr_buf[start..end] {
            if top.is_none() {
                top = Some(row);
            }
            bottom = Some(row);
        }
    }

    let (top, bottom) = (top?, bottom?);
    Some((0, top as u32, width, (bottom - top + 1) as u32))
}
