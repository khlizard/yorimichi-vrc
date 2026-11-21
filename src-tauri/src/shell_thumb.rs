//! Thumbnails from the Windows Shell thumbnail pipeline, used for online-only (cloud placeholder)
//! files. The sync provider supplies the thumbnail, so the file content is never opened here.

use std::path::Path;

#[cfg(not(windows))]
pub fn shell_thumbnail_jpeg(_path: &Path, _max_edge: u32) -> Option<Vec<u8>> {
    None
}

#[cfg(windows)]
pub fn shell_thumbnail_jpeg(path: &Path, max_edge: u32) -> Option<Vec<u8>> {
    win::shell_thumbnail_jpeg(path, max_edge)
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use image::codecs::jpeg::JpegEncoder;
    use image::{imageops, ExtendedColorType, RgbImage};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_THUMBNAILONLY,
    };

    const BACKGROUND: [u32; 3] = [0x1b, 0x1b, 0x1f];
    const JPEG_QUALITY: u8 = 82;

    struct ComGuard(bool);
    impl ComGuard {
        fn new() -> Self {
            // S_OK / S_FALSE need a matching CoUninitialize; a failure such as RPC_E_CHANGED_MODE does not.
            // SAFETY: plain COM initialisation of the current thread; the result decides whether `drop` uninitialises.
            Self(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: pairs the successful CoInitializeEx of `new` on this same thread.
                unsafe { CoUninitialize() };
            }
        }
    }

    struct Bitmap(HBITMAP);
    impl Drop for Bitmap {
        fn drop(&mut self) {
            // SAFETY: the wrapped bitmap was returned by `GetImage`, is owned by this guard and is deleted once.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.0 .0));
            }
        }
    }

    pub fn shell_thumbnail_jpeg(path: &Path, max_edge: u32) -> Option<Vec<u8>> {
        let _com = ComGuard::new();
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call; `bmp` owns the returned bitmap for
        // `read_bitmap`, and COM stays initialised on this thread through `_com`.
        let img = unsafe {
            let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
            let size = SIZE { cx: max_edge as i32, cy: max_edge as i32 };
            let bmp = Bitmap(factory.GetImage(size, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK).ok()?);
            read_bitmap(&bmp)?
        };
        encode(img, max_edge)
    }

    /// # Safety
    /// `bmp` must hold a valid GDI bitmap handle.
    unsafe fn read_bitmap(bmp: &Bitmap) -> Option<RgbImage> {
        // SAFETY: `info` and `bi` are valid, correctly sized out-structures; `buf` holds `w * h * 4`
        // bytes, exactly what `GetDIBits` writes for `h` lines of 32-bit pixels; the DC is released below.
        unsafe {
            let mut info = BITMAP::default();
            let got = GetObjectW(HGDIOBJ(bmp.0 .0), size_of::<BITMAP>() as i32, Some(&mut info as *mut _ as *mut _));
            if got == 0 || info.bmWidth <= 0 || info.bmHeight <= 0 {
                return None;
            }
            let (w, h) = (info.bmWidth as usize, info.bmHeight as usize);
            if w > 16384 || h > 16384 {
                return None;
            }
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut buf = vec![0u8; w * h * 4];
            let hdc = GetDC(None);
            let lines = GetDIBits(hdc, bmp.0, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
            ReleaseDC(None, hdc);
            if lines as usize != h {
                return None;
            }
            // Premultiplied alpha; an all-zero alpha channel means the bitmap carries no alpha.
            let has_alpha = buf.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
            let mut rgb = Vec::with_capacity(w * h * 3);
            for p in buf.as_chunks::<4>().0 {
                let (b, g, r) = (p[0] as u32, p[1] as u32, p[2] as u32);
                if has_alpha {
                    let inv = 255 - p[3] as u32;
                    for (c, bg) in [r, g, b].into_iter().zip(BACKGROUND) {
                        rgb.push((c + (bg * inv + 127) / 255).min(255) as u8);
                    }
                } else {
                    rgb.extend_from_slice(&[r as u8, g as u8, b as u8]);
                }
            }
            RgbImage::from_raw(w as u32, h as u32, rgb)
        }
    }

    fn encode(img: RgbImage, max_edge: u32) -> Option<Vec<u8>> {
        let img = if img.width() > max_edge || img.height() > max_edge {
            let scale = max_edge as f64 / img.width().max(img.height()) as f64;
            let nw = ((img.width() as f64 * scale).round() as u32).max(1);
            let nh = ((img.height() as f64 * scale).round() as u32).max(1);
            imageops::resize(&img, nw, nh, imageops::FilterType::Triangle)
        } else {
            img
        };
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .encode(img.as_raw(), img.width(), img.height(), ExtendedColorType::Rgb8)
            .ok()?;
        Some(out)
    }
}
