//! Safe RAII wrappers around system `libopenjp2` (JPEG 2000) and `libheif` (HEIC/HEIF).

#[cfg(has_openjp2)]
pub mod jp2 {
    use std::ffi::{CString, c_char, c_int, c_void};
    use std::fs::File;
    use std::io::Read;
    use std::path::Path;
    use std::ptr;

    use image::RgbaImage;

    use crate::ImageError;

    const OPJ_PATH_LEN: usize = 4096;
    const OPJ_CODEC_J2K: c_int = 0;
    const OPJ_CODEC_JP2: c_int = 2;

    #[repr(C)]
    struct OpjDParameters {
        cp_reduce: u32,
        cp_layer: u32,
        infile: [c_char; OPJ_PATH_LEN],
        outfile: [c_char; OPJ_PATH_LEN],
        decod_format: c_int,
        cod_format: c_int,
        da_x0: u32,
        da_x1: u32,
        da_y0: u32,
        da_y1: u32,
        m_verbose: c_int,
        tile_index: u32,
        nb_tile_to_decode: u32,
        jpwl_correct: c_int,
        jpwl_exp_comps: c_int,
        jpwl_max_tiles: c_int,
        flags: u32,
    }

    #[repr(C)]
    struct OpjImageComp {
        dx: u32,
        dy: u32,
        w: u32,
        h: u32,
        x0: u32,
        y0: u32,
        prec: u32,
        bpp: u32,
        sgnd: u32,
        resno_decoded: u32,
        factor: u32,
        data: *mut i32,
        alpha: u16,
    }

    #[repr(C)]
    struct OpjImage {
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        numcomps: u32,
        color_space: c_int,
        comps: *mut OpjImageComp,
        icc_profile_buf: *mut u8,
        icc_profile_len: u32,
    }

    unsafe extern "C" {
        fn opj_stream_create_default_file_stream(
            fname: *const c_char,
            p_is_read_stream: c_int,
        ) -> *mut c_void;
        fn opj_stream_destroy(p_stream: *mut c_void);
        fn opj_create_decompress(format: c_int) -> *mut c_void;
        fn opj_destroy_codec(p_codec: *mut c_void);
        fn opj_set_default_decoder_parameters(parameters: *mut OpjDParameters);
        fn opj_setup_decoder(p_codec: *mut c_void, parameters: *mut OpjDParameters) -> c_int;
        fn opj_read_header(
            p_stream: *mut c_void,
            p_codec: *mut c_void,
            p_image: *mut *mut OpjImage,
        ) -> c_int;
        fn opj_decode(
            p_decompressor: *mut c_void,
            p_stream: *mut c_void,
            p_image: *mut OpjImage,
        ) -> c_int;
        fn opj_end_decompress(p_codec: *mut c_void, p_stream: *mut c_void) -> c_int;
        fn opj_image_destroy(image: *mut OpjImage);
    }

    struct OpjStream(*mut c_void);
    impl Drop for OpjStream {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { opj_stream_destroy(self.0) };
            }
        }
    }

    struct OpjCodec(*mut c_void);
    impl Drop for OpjCodec {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { opj_destroy_codec(self.0) };
            }
        }
    }

    struct OpjImg(*mut OpjImage);
    impl Drop for OpjImg {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { opj_image_destroy(self.0) };
            }
        }
    }

    fn detect_codec_format(path: &Path) -> c_int {
        if let Ok(mut f) = File::open(path) {
            let mut magic = [0u8; 12];
            if let Ok(n) = f.read(&mut magic) {
                if n >= 12
                    && magic
                        == [
                            0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
                        ]
                {
                    return OPJ_CODEC_JP2;
                }
                if n >= 4 && magic[..4] == [0xFF, 0x4F, 0xFF, 0x51] {
                    return OPJ_CODEC_J2K;
                }
            }
        }
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("j2k" | "j2c" | "jpc") => OPJ_CODEC_J2K,
            _ => OPJ_CODEC_JP2,
        }
    }

    fn path_cstring(path: &Path) -> Option<CString> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            CString::new(path.as_os_str().as_bytes()).ok()
        }
        #[cfg(not(unix))]
        {
            CString::new(path.to_str()?.as_bytes()).ok()
        }
    }

    fn open_header(path: &Path) -> Result<(OpjStream, OpjCodec, OpjImg), String> {
        let c_path = path_cstring(path).ok_or_else(|| "invalid file path".to_owned())?;
        let stream =
            OpjStream(unsafe { opj_stream_create_default_file_stream(c_path.as_ptr(), 1) });
        if stream.0.is_null() {
            return Err("failed to open JPEG 2000 stream".to_owned());
        }
        let codec = OpjCodec(unsafe { opj_create_decompress(detect_codec_format(path)) });
        if codec.0.is_null() {
            return Err("failed to create OpenJPEG decoder".to_owned());
        }
        let mut params = std::mem::MaybeUninit::<OpjDParameters>::zeroed();
        unsafe {
            opj_set_default_decoder_parameters(params.as_mut_ptr());
            if opj_setup_decoder(codec.0, params.as_mut_ptr()) == 0 {
                return Err("opj_setup_decoder failed".to_owned());
            }
        }
        let mut raw_img: *mut OpjImage = ptr::null_mut();
        if unsafe { opj_read_header(stream.0, codec.0, &mut raw_img) } == 0 || raw_img.is_null() {
            return Err("opj_read_header failed".to_owned());
        }
        Ok((stream, codec, OpjImg(raw_img)))
    }

    pub fn probe_dimensions(path: &Path) -> Option<(u32, u32)> {
        let (_stream, _codec, img) = open_header(path).ok()?;
        let header = unsafe { &*img.0 };
        let w = header.x1.checked_sub(header.x0)?;
        let h = header.y1.checked_sub(header.y0)?;
        (w > 0 && h > 0).then_some((w, h))
    }

    #[inline]
    fn sample_to_u8(val: i32, prec: u32, sgnd: u32) -> u8 {
        if prec == 0 {
            return 0;
        }
        let shifted = if sgnd != 0 {
            val.saturating_add(1i32.checked_shl(prec.saturating_sub(1)).unwrap_or(0))
        } else {
            val
        };
        let clamped = shifted.max(0) as u32;
        if prec == 8 {
            clamped.min(255) as u8
        } else if prec < 8 {
            let max_in = (1u32 << prec) - 1;
            ((clamped.min(max_in) * 255 + max_in / 2) / max_in) as u8
        } else {
            let shift = prec - 8;
            (clamped >> shift).min(255) as u8
        }
    }

    #[inline]
    unsafe fn read_comp_u8(comp: &OpjImageComp, x: u32, y: u32, w: u32, h: u32) -> u8 {
        if comp.data.is_null() || comp.w == 0 || comp.h == 0 {
            return 0;
        }
        let cx = if comp.w == w {
            x
        } else {
            ((u64::from(x) * u64::from(comp.w)) / u64::from(w.max(1))) as u32
        }
        .min(comp.w - 1);
        let cy = if comp.h == h {
            y
        } else {
            ((u64::from(y) * u64::from(comp.h)) / u64::from(h.max(1))) as u32
        }
        .min(comp.h - 1);
        let idx = (cy as usize) * (comp.w as usize) + (cx as usize);
        let val = unsafe { *comp.data.add(idx) };
        sample_to_u8(val, comp.prec, comp.sgnd)
    }

    pub fn decode_rgba(path: &Path) -> Result<RgbaImage, ImageError> {
        let map_err = |msg: String| ImageError::Decode {
            path: path.display().to_string(),
            message: msg,
        };
        let (stream, codec, img) = open_header(path).map_err(map_err)?;
        let (w, h) = unsafe {
            let hdr = &*img.0;
            let w = hdr.x1.saturating_sub(hdr.x0);
            let h = hdr.y1.saturating_sub(hdr.y0);
            (w, h)
        };
        crate::validate_dimensions(w, h)?;

        if unsafe { opj_decode(codec.0, stream.0, img.0) } == 0 {
            return Err(map_err("opj_decode failed".to_owned()));
        }
        let _ = unsafe { opj_end_decompress(codec.0, stream.0) };

        let image_ref = unsafe { &*img.0 };
        let numcomps = image_ref.numcomps as usize;
        if numcomps == 0 || image_ref.comps.is_null() {
            return Err(map_err("JPEG 2000 image has no components".to_owned()));
        }
        let comps = unsafe { std::slice::from_raw_parts(image_ref.comps, numcomps) };

        let mut rgba = vec![0u8; (w as usize).saturating_mul(h as usize).saturating_mul(4)];
        for y in 0..h {
            for x in 0..w {
                let off = ((y as usize) * (w as usize) + (x as usize)) * 4;
                let (r, g, b, a) = unsafe {
                    match numcomps {
                        1 => {
                            let l = read_comp_u8(&comps[0], x, y, w, h);
                            (l, l, l, 255)
                        }
                        2 => {
                            let l = read_comp_u8(&comps[0], x, y, w, h);
                            let a = read_comp_u8(&comps[1], x, y, w, h);
                            (l, l, l, a)
                        }
                        3 => (
                            read_comp_u8(&comps[0], x, y, w, h),
                            read_comp_u8(&comps[1], x, y, w, h),
                            read_comp_u8(&comps[2], x, y, w, h),
                            255,
                        ),
                        _ => (
                            read_comp_u8(&comps[0], x, y, w, h),
                            read_comp_u8(&comps[1], x, y, w, h),
                            read_comp_u8(&comps[2], x, y, w, h),
                            read_comp_u8(&comps[3], x, y, w, h),
                        ),
                    }
                };
                rgba[off] = r;
                rgba[off + 1] = g;
                rgba[off + 2] = b;
                rgba[off + 3] = a;
            }
        }

        RgbaImage::from_raw(w, h, rgba).ok_or_else(|| map_err("JP2 RGBA size mismatch".to_owned()))
    }
}

#[cfg(has_libheif)]
pub mod heif {
    use std::ffi::{CStr, c_char, c_int, c_void};
    use std::fs;
    use std::path::Path;
    use std::ptr;

    use image::RgbaImage;

    use crate::ImageError;

    const HEIF_COLORSPACE_RGB: c_int = 1;
    const HEIF_CHROMA_INTERLEAVED_RGBA: c_int = 11;
    const HEIF_CHANNEL_INTERLEAVED: c_int = 10;

    #[repr(C)]
    struct HeifError {
        code: c_int,
        subcode: c_int,
        message: *const c_char,
    }

    impl HeifError {
        fn into_result(self) -> Result<(), String> {
            if self.code == 0 {
                return Ok(());
            }
            let msg = if self.message.is_null() {
                format!("libheif error {}", self.code)
            } else {
                unsafe { CStr::from_ptr(self.message) }
                    .to_string_lossy()
                    .into_owned()
            };
            Err(msg)
        }
    }

    unsafe extern "C" {
        fn heif_context_alloc() -> *mut c_void;
        fn heif_context_free(ctx: *mut c_void);
        fn heif_context_read_from_memory_without_copy(
            ctx: *mut c_void,
            mem: *const c_void,
            size: usize,
            options: *const c_void,
        ) -> HeifError;
        fn heif_context_get_primary_image_handle(
            ctx: *mut c_void,
            out_handle: *mut *mut c_void,
        ) -> HeifError;
        fn heif_image_handle_release(handle: *const c_void);
        fn heif_image_handle_get_width(handle: *const c_void) -> c_int;
        fn heif_image_handle_get_height(handle: *const c_void) -> c_int;
        fn heif_decode_image(
            in_handle: *const c_void,
            out_img: *mut *mut c_void,
            colorspace: c_int,
            chroma: c_int,
            options: *const c_void,
        ) -> HeifError;
        fn heif_image_get_plane_readonly(
            img: *const c_void,
            channel: c_int,
            out_stride: *mut c_int,
        ) -> *const u8;
        fn heif_image_release(img: *const c_void);
    }

    struct HeifCtx(*mut c_void);
    impl Drop for HeifCtx {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { heif_context_free(self.0) };
            }
        }
    }

    struct HeifHandle(*mut c_void);
    impl Drop for HeifHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { heif_image_handle_release(self.0) };
            }
        }
    }

    struct HeifImg(*mut c_void);
    impl Drop for HeifImg {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { heif_image_release(self.0) };
            }
        }
    }

    fn open_primary_handle(bytes: &[u8]) -> Result<(HeifCtx, HeifHandle), String> {
        let ctx = HeifCtx(unsafe { heif_context_alloc() });
        if ctx.0.is_null() {
            return Err("heif_context_alloc failed".to_owned());
        }
        unsafe {
            heif_context_read_from_memory_without_copy(
                ctx.0,
                bytes.as_ptr().cast(),
                bytes.len(),
                ptr::null(),
            )
        }
        .into_result()?;
        let mut raw_handle: *mut c_void = ptr::null_mut();
        unsafe { heif_context_get_primary_image_handle(ctx.0, &mut raw_handle) }.into_result()?;
        if raw_handle.is_null() {
            return Err("missing primary HEIF image handle".to_owned());
        }
        Ok((ctx, HeifHandle(raw_handle)))
    }

    pub fn probe_dimensions(path: &Path) -> Option<(u32, u32)> {
        let bytes = fs::read(path).ok()?;
        let (_ctx, handle) = open_primary_handle(&bytes).ok()?;
        let w = unsafe { heif_image_handle_get_width(handle.0) };
        let h = unsafe { heif_image_handle_get_height(handle.0) };
        (w > 0 && h > 0).then_some((w as u32, h as u32))
    }

    pub fn decode_rgba(path: &Path) -> Result<RgbaImage, ImageError> {
        let bytes = fs::read(path).map_err(|source| ImageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let map_err = |msg: String| ImageError::Decode {
            path: path.display().to_string(),
            message: msg,
        };
        let (_ctx, handle) = open_primary_handle(&bytes).map_err(map_err)?;
        let w_i = unsafe { heif_image_handle_get_width(handle.0) };
        let h_i = unsafe { heif_image_handle_get_height(handle.0) };
        if w_i <= 0 || h_i <= 0 {
            return Err(map_err("invalid HEIF image dimensions".to_owned()));
        }
        let (w, h) = (w_i as u32, h_i as u32);
        crate::validate_dimensions(w, h)?;

        let mut raw_img: *mut c_void = ptr::null_mut();
        unsafe {
            heif_decode_image(
                handle.0,
                &mut raw_img,
                HEIF_COLORSPACE_RGB,
                HEIF_CHROMA_INTERLEAVED_RGBA,
                ptr::null(),
            )
        }
        .into_result()
        .map_err(map_err)?;
        if raw_img.is_null() {
            return Err(map_err("heif_decode_image returned null".to_owned()));
        }
        let img = HeifImg(raw_img);

        let mut stride_c: c_int = 0;
        let plane = unsafe {
            heif_image_get_plane_readonly(img.0, HEIF_CHANNEL_INTERLEAVED, &mut stride_c)
        };
        let stride = usize::try_from(stride_c).unwrap_or(0);
        let row_bytes = (w as usize).saturating_mul(4);
        if plane.is_null() || stride < row_bytes {
            return Err(map_err("invalid HEIF RGBA plane".to_owned()));
        }

        let mut rgba = vec![0u8; row_bytes.saturating_mul(h as usize)];
        for y in 0..(h as usize) {
            let src = unsafe { std::slice::from_raw_parts(plane.add(y * stride), row_bytes) };
            rgba[y * row_bytes..(y + 1) * row_bytes].copy_from_slice(src);
        }

        RgbaImage::from_raw(w, h, rgba).ok_or_else(|| map_err("HEIF RGBA size mismatch".to_owned()))
    }
}
