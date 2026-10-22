//! Screen capture of the monitor under the cursor, kept only in memory.

use image::RgbaImage;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

pub struct Shot {
    pub img: RgbaImage,
    /// Pre-encoded BMP served to the overlay through the `gtshot` protocol.
    pub bmp: Vec<u8>,
    pub mon_x: i32,
    pub mon_y: i32,
    pub mon_w: u32,
    pub mon_h: u32,
}

impl Shot {
    /// Overwrite pixel buffers before they are freed.
    pub fn wipe(&mut self) {
        self.bmp.iter_mut().for_each(|b| *b = 0);
        self.img.as_mut().iter_mut().for_each(|b| *b = 0);
        self.bmp.clear();
    }
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

pub fn capture_cursor_monitor() -> Result<Shot, String> {
    let (cx, cy) = cursor_pos();
    let mon = match xcap::Monitor::from_point(cx, cy) {
        Ok(m) => m,
        Err(_) => xcap::Monitor::all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| "no monitor found".to_string())?,
    };
    let img = mon.capture_image().map_err(|e| e.to_string())?;
    let mon_x = mon.x().map_err(|e| e.to_string())?;
    let mon_y = mon.y().map_err(|e| e.to_string())?;
    let mon_w = mon.width().map_err(|e| e.to_string())?;
    let mon_h = mon.height().map_err(|e| e.to_string())?;
    if img.width() == 0 || img.height() == 0 {
        return Err("empty capture".into());
    }
    let bmp = encode_bmp(&img);
    Ok(Shot { img, bmp, mon_x, mon_y, mon_w, mon_h })
}

/// Uncompressed top-down 32-bit BMP (BGRA). Fast and dependency free.
fn encode_bmp(img: &RgbaImage) -> Vec<u8> {
    let (w, h) = (img.width(), img.height());
    let data_len = (w as usize) * (h as usize) * 4;
    let mut out = Vec::with_capacity(54 + data_len);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for px in img.as_raw().chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    out
}
